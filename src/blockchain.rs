//! Oasis blockchain simulator.
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
};

use crate::{
    confidential::ConfidentialCtx, genesis, parity::NullBackend, storage::MemoryMKVS, util,
    ExecutionResult, BLOCK_GAS_LIMIT, MIN_GAS_PRICE_GWEI,
};
use ekiden_keymanager::client::MockClient;
use ethcore::{
    error::CallError,
    executive::{contract_address, Executed, Executive, TransactOptions},
    filter::Filter,
    log_entry::LocalizedLogEntry,
    receipt::{LocalizedReceipt, TransactionOutcome},
    state::State,
    transaction::{Action, LocalizedTransaction, SignedTransaction, UnverifiedTransaction},
    types::ids::BlockId,
    vm::EnvInfo,
};
use ethereum_types::{Bloom, H256, H64, U256};
use failure::{format_err, Error, Fallible};
use futures::{future, prelude::*};
use hash::keccak;
use lazy_static::lazy_static;
use parity_rpc::v1::types::{
    Block as EthRpcBlock, BlockTransactions as EthRpcBlockTransactions, Header as EthRpcHeader,
    RichBlock as EthRpcRichBlock, RichHeader as EthRpcRichHeader, Transaction as EthRpcTransaction,
};
use tokio_threadpool::{Builder as ThreadPoolBuilder, ThreadPool};

/// Boxed future type.
type BoxFuture<T> = Box<dyn futures::Future<Item = T, Error = failure::Error> + Send>;

/// Simulated blockchain.
pub struct Blockchain {
    gas_price: U256,
    simulator_pool: Arc<ThreadPool>,
    km_client: Arc<MockClient>,
    chain_state: Arc<RwLock<ChainState>>,
}

/// Simulated blockchain state.
pub struct ChainState {
    mkvs: MemoryMKVS,
    block_number: u64,
    blocks: HashMap<H256, EthereumBlock>,
    block_number_to_hash: HashMap<u64, H256>,
    transactions: HashMap<H256, LocalizedTransaction>,
    receipts: HashMap<H256, LocalizedReceipt>,
}

impl Blockchain {
    /// Create new simulated blockchain.
    pub fn new(gas_price: U256, km_client: Arc<MockClient>) -> Self {
        // Initialize genesis state.
        let mkvs = MemoryMKVS::new();
        genesis::SPEC
            .ensure_db_good(Box::new(mkvs.clone()), NullBackend, &Default::default())
            .expect("genesis initialization must succeed");

        // Initialize chain state.
        let block_number = 0;
        let mut blocks = HashMap::new();
        let mut block_number_to_hash = HashMap::new();
        let genesis_block = EthereumBlock::new(
            block_number,
            0,
            U256::from(0),
            BLOCK_GAS_LIMIT.into(),
            Default::default(),
        );
        let block_hash = genesis_block.hash();
        blocks.insert(block_hash, genesis_block);
        block_number_to_hash.insert(block_number, block_hash);

        let chain_state = ChainState {
            block_number,
            blocks,
            block_number_to_hash,
            receipts: HashMap::new(),
            transactions: HashMap::new(),
            mkvs: mkvs,
        };

        Self {
            gas_price,
            simulator_pool: Arc::new(
                ThreadPoolBuilder::new()
                    .name_prefix("simulator-pool-")
                    .build(),
            ),
            km_client,
            chain_state: Arc::new(RwLock::new(chain_state)),
        }
    }

    /// Ethereum state snapshot at given block.
    pub fn state(&self, _id: BlockId) -> Fallible<State<NullBackend>> {
        let chain_state = self.chain_state.read().unwrap();

        // TODO: support previous block states
        Ok(State::from_existing(
            Box::new(chain_state.mkvs.clone()),
            NullBackend,
            U256::zero(),       /* account_start_nonce */
            Default::default(), /* factories */
            None,               /* confidential_ctx */
        )?)
    }

    /// Gas price.
    pub fn gas_price(&self) -> U256 {
        self.gas_price
    }

    /// Retrieve an Ethereum block given a block identifier.
    pub fn get_block(
        &self,
        id: BlockId,
    ) -> impl Future<Item = Option<EthereumBlock>, Error = Error> {
        let block: BoxFuture<Option<EthereumBlock>> = match id {
            BlockId::Hash(hash) => Box::new(self.get_block_by_hash(hash)),
            BlockId::Number(number) => Box::new(self.get_block_by_number(number)),
            BlockId::Latest => Box::new(self.get_latest_block().map(|blk| Some(blk))),
            BlockId::Earliest => Box::new(self.get_block_by_number(0)),
        };

        block
    }

    /// The current best block number.
    fn best_block_number(&self) -> u64 {
        let chain_state = self.chain_state.read().unwrap();
        chain_state.block_number
    }

    /// Retrieve the latest Ethereum block.
    pub fn get_latest_block(&self) -> impl Future<Item = EthereumBlock, Error = Error> {
        let chain_state = self.chain_state.read().unwrap();

        let hash = chain_state
            .block_number_to_hash
            .get(&chain_state.block_number)
            .expect("best block must exist");

        future::ok(
            chain_state
                .blocks
                .get(hash)
                .expect("best block must exist")
                .clone(),
        )
    }

    /// Retrieve a specific Ethereum block, identified by its number.
    pub fn get_block_by_number(
        &self,
        number: u64,
    ) -> impl Future<Item = Option<EthereumBlock>, Error = Error> {
        let chain_state = self.chain_state.read().unwrap();

        future::ok(
            chain_state
                .block_number_to_hash
                .get(&number)
                .and_then(|hash| chain_state.blocks.get(hash))
                .cloned(),
        )
    }

    /// Retrieve a specific Ethereum block, identified by its block hash.
    pub fn get_block_by_hash(
        &self,
        hash: H256,
    ) -> impl Future<Item = Option<EthereumBlock>, Error = Error> {
        let chain_state = self.chain_state.read().unwrap();

        future::ok(chain_state.blocks.get(&hash).cloned())
    }

    /// Retrieve a specific Ethereum transaction, identified by its transaction hash.
    pub fn get_txn_by_hash(
        &self,
        hash: H256,
    ) -> impl Future<Item = Option<LocalizedTransaction>, Error = Error> {
        let chain_state = self.chain_state.read().unwrap();

        future::ok(chain_state.transactions.get(&hash).cloned())
    }

    /// Retrieve a specific Ethereum transaction receipt, identified by its transaction
    /// hash.
    pub fn get_txn_receipt_by_hash(
        &self,
        hash: H256,
    ) -> impl Future<Item = Option<LocalizedReceipt>, Error = Error> {
        let chain_state = self.chain_state.read().unwrap();

        future::ok(chain_state.receipts.get(&hash).cloned())
    }

    /// Retrieve a specific Ethereum transaction, identified by the block round and
    /// transaction index within the block.
    pub fn get_txn_by_number_and_index(
        &self,
        number: u64,
        index: u32,
    ) -> impl Future<Item = Option<LocalizedTransaction>, Error = Error> {
        let chain_state = self.chain_state.read().unwrap();

        future::ok(
            chain_state
                .block_number_to_hash
                .get(&number)
                .and_then(|hash| chain_state.blocks.get(hash))
                .and_then(|blk| blk.transactions.get(index as usize))
                .cloned(),
        )
    }

    /// Retrieve a specific Ethereum transaction, identified by the block hash and
    /// transaction index within the block.
    pub fn get_txn_by_block_hash_and_index(
        &self,
        block_hash: H256,
        index: u32,
    ) -> impl Future<Item = Option<LocalizedTransaction>, Error = Error> {
        let chain_state = self.chain_state.read().unwrap();

        future::ok(
            chain_state
                .blocks
                .get(&block_hash)
                .and_then(|blk| blk.transactions.get(index as usize))
                .cloned(),
        )
    }

    /// Retrieve a specific Ethereum transaction, identified by a block identifier
    /// and transaction index within the block.
    pub fn get_txn(
        &self,
        id: BlockId,
        index: u32,
    ) -> impl Future<Item = Option<LocalizedTransaction>, Error = Error> {
        let txn: BoxFuture<Option<LocalizedTransaction>> = match id {
            BlockId::Hash(hash) => Box::new(self.get_txn_by_block_hash_and_index(hash, index)),
            BlockId::Number(number) => Box::new(self.get_txn_by_number_and_index(number, index)),
            BlockId::Latest => {
                Box::new(self.get_txn_by_number_and_index(self.best_block_number(), index))
            }
            BlockId::Earliest => Box::new(self.get_txn_by_number_and_index(0, index)),
        };

        txn
    }

    /// Submit a raw Ethereum transaction to the chain.
    pub fn send_raw_transaction(
        &self,
        raw: Vec<u8>,
    ) -> impl Future<Item = (H256, ExecutionResult), Error = Error> {
        // Decode transaction.
        let decoded: UnverifiedTransaction = match rlp::decode(&raw) {
            Ok(t) => t,
            Err(_) => return Err(format_err!("Could not decode transaction")).into_future(),
        };

        // Check that gas < block gas limit.
        if decoded.as_unsigned().gas > BLOCK_GAS_LIMIT.into() {
            return Err(format_err!("Requested gas greater than block gas limit")).into_future();
        }

        // Check signature.
        let txn = match SignedTransaction::new(decoded.clone()) {
            Ok(t) => t,
            Err(_) => return Err(format_err!("Invalid signature")).into_future(),
        };

        // Check gas price.
        if txn.gas_price < MIN_GAS_PRICE_GWEI.into() {
            return Err(format_err!("Insufficient gas price")).into_future();
        }

        // Mine a block with the transaction.
        future::done(self.mine_block(txn))
    }

    /// Mine a block containing the transaction.
    fn mine_block(&self, txn: SignedTransaction) -> Result<(H256, ExecutionResult), Error> {
        let mut chain_state = self.chain_state.write().unwrap();

        // Initialize Ethereum state access functions.
        // TODO: previous block hash
        let mut state = State::from_existing(
            Box::new(chain_state.mkvs.clone()),
            NullBackend,
            U256::zero(),       /* account_start_nonce */
            Default::default(), /* factories */
            Some(Box::new(ConfidentialCtx::new(
                Default::default(),
                self.km_client.clone(),
            ))),
        )
        .expect("state initialization must succeed");

        // Initialize Ethereum environment information.
        let number = chain_state.block_number + 1;
        let timestamp = util::get_timestamp();
        let env_info = EnvInfo {
            number,
            author: Default::default(),
            timestamp,
            difficulty: Default::default(),
            gas_limit: *genesis::GAS_LIMIT,
            // TODO: Get 256 last_hashes.
            last_hashes: Arc::new(vec![]),
            gas_used: Default::default(),
        };

        // Execute the transaction.
        let outcome =
            match state.apply(&env_info, genesis::SPEC.engine.machine(), &txn, false, true) {
                Ok(outcome) => outcome,
                Err(err) => return Err(format_err!("{}", err)),
            };

        // Commit the state updates.
        state.commit().expect("state commit must succeed");

        // Create a block.
        let mut block = EthereumBlock::new(
            number,
            timestamp,
            outcome.receipt.gas_used,
            BLOCK_GAS_LIMIT.into(),
            outcome.receipt.log_bloom,
        );
        let block_hash = block.hash();
        chain_state.block_number = number;

        // Store the txn.
        let txn_hash = txn.hash();
        let localized_txn = LocalizedTransaction {
            signed: txn.clone().into(),
            block_number: number,
            block_hash,
            transaction_index: 0,
            cached_sender: None,
        };
        block.add_transaction(localized_txn.clone());
        chain_state.transactions.insert(txn_hash, localized_txn);

        // Store the receipt.
        let localized_receipt = LocalizedReceipt {
            transaction_hash: txn_hash,
            transaction_index: 0,
            block_hash: block_hash,
            block_number: number,
            cumulative_gas_used: outcome.receipt.gas_used,
            gas_used: outcome.receipt.gas_used,
            contract_address: match txn.action {
                Action::Call(_) => None,
                Action::Create => Some(
                    contract_address(
                        genesis::SPEC.engine.create_address_scheme(number),
                        &txn.sender(),
                        &txn.nonce,
                        &txn.data,
                    )
                    .0,
                ),
            },
            logs: outcome
                .receipt
                .logs
                .clone()
                .into_iter()
                .enumerate()
                .map(|(i, log)| LocalizedLogEntry {
                    entry: log,
                    block_hash: block_hash,
                    block_number: number,
                    transaction_hash: txn_hash,
                    transaction_index: 0,
                    transaction_log_index: i,
                    log_index: i,
                })
                .collect(),
            log_bloom: outcome.receipt.log_bloom,
            outcome: outcome.receipt.outcome.clone(),
        };
        chain_state.receipts.insert(txn_hash, localized_receipt);

        // Store the block.
        chain_state.blocks.insert(block_hash, block.clone());
        chain_state.block_number_to_hash.insert(number, block_hash);

        // Return the ExecutionResult.
        let result = ExecutionResult {
            cumulative_gas_used: outcome.receipt.gas_used,
            gas_used: outcome.receipt.gas_used,
            log_bloom: outcome.receipt.log_bloom,
            logs: outcome.receipt.logs,
            status_code: match outcome.receipt.outcome {
                TransactionOutcome::StatusCode(code) => code,
                _ => unreachable!("we always use EIP-658 semantics"),
            },
            output: outcome.output.into(),
        };

        info!(
            "Mined block number {:?} containing transaction {:?}",
            number, txn_hash
        );

        Ok((txn_hash, result))
    }

    /// Simulate a transaction against a given block.
    ///
    /// The simulated transaction is executed in a dedicated thread pool to
    /// avoid blocking I/O processing.
    ///
    /// # Notes
    ///
    /// Confidential contracts are not supported.
    pub fn simulate_transaction(
        &self,
        transaction: SignedTransaction,
        _id: BlockId,
    ) -> impl Future<Item = Executed, Error = CallError> {
        let simulator_pool = self.simulator_pool.clone();
        let chain_state = self.chain_state.clone();

        // Execute simulation in a dedicated thread pool to avoid blocking
        // I/O processing with simulations.
        simulator_pool.spawn_handle(future::lazy(move || {
            let chain_state = chain_state.read().unwrap();

            let env_info = EnvInfo {
                number: chain_state.block_number + 1,
                author: Default::default(),
                timestamp: util::get_timestamp(),
                difficulty: Default::default(),
                // TODO: Get 256 last hashes.
                last_hashes: Arc::new(vec![]),
                gas_used: Default::default(),
                gas_limit: U256::max_value(),
            };
            let machine = genesis::SPEC.engine.machine();
            let options = TransactOptions::with_no_tracing()
                .dont_check_nonce()
                .save_output_from_contract();
            let mut state = State::from_existing(
                Box::new(chain_state.mkvs.clone()),
                NullBackend,
                U256::zero(),       /* account_start_nonce */
                Default::default(), /* factories */
                None,               /* confidential_ctx */
            )
            .expect("state initialization must succeed");

            Ok(Executive::new(&mut state, &env_info, machine)
                .transact_virtual(&transaction, options)?)
        }))
    }

    /// Estimates gas against a given block.
    ///
    /// Uses `simulate_transaction` internally.
    ///
    /// # Notes
    ///
    /// Confidential contracts are not supported.
    pub fn estimate_gas(
        &self,
        transaction: SignedTransaction,
        id: BlockId,
    ) -> impl Future<Item = U256, Error = CallError> {
        self.simulate_transaction(transaction, id)
            .map(|executed| executed.gas_used + executed.refunded)
    }

    /// Looks up logs based on the given filter.
    pub fn logs(
        &self,
        _filter: Filter,
    ) -> impl Future<Item = Vec<LocalizedLogEntry>, Error = Error> {
        // TODO: implement
        Err(format_err!("not implemented")).into_future()
    }
}

lazy_static! {
    // dummy-valued PoW-related block extras
    static ref BLOCK_EXTRA_INFO: BTreeMap<String, String> = {
        let mut map = BTreeMap::new();
        map.insert("mixHash".into(), format!("0x{:x}", H256::default()));
        map.insert("nonce".into(), format!("0x{:x}", H64::default()));
        map
    };
}

/// A wrapper that exposes a simulated Ethereum block.
#[derive(Clone, Debug)]
pub struct EthereumBlock {
    number: u64,
    timestamp: u64,
    hash: H256,
    gas_used: U256,
    gas_limit: U256,
    log_bloom: Bloom,
    transactions: Vec<LocalizedTransaction>,
}

impl EthereumBlock {
    /// Create a new Ethereum block.
    pub fn new(
        number: u64,
        timestamp: u64,
        gas_used: U256,
        gas_limit: U256,
        log_bloom: Bloom,
    ) -> Self {
        // TODO: better blockhash
        Self {
            number,
            timestamp,
            transactions: vec![],
            hash: keccak(number.to_string()).into(),
            gas_used,
            gas_limit,
            log_bloom,
        }
    }

    /// Ethereum block number.
    pub fn number(&self) -> U256 {
        U256::from(self.number)
    }

    /// Ethereum block number as an u64.
    pub fn number_u64(&self) -> u64 {
        self.number
    }

    /// Block hash.
    pub fn hash(&self) -> H256 {
        self.hash
    }

    // Ethereum transactions contained in the block.
    pub fn transactions(&self) -> Vec<LocalizedTransaction> {
        self.transactions.clone()
    }

    pub fn rich_header(&self) -> EthRpcRichHeader {
        EthRpcRichHeader {
            inner: EthRpcHeader {
                hash: Some(self.hash.into()),
                size: None,
                // TODO: parent hash
                parent_hash: Default::default(),
                uncles_hash: Default::default(),
                author: Default::default(),
                miner: Default::default(),
                // TODO: state root
                state_root: Default::default(),
                transactions_root: Default::default(),
                receipts_root: Default::default(),
                number: Some(self.number.into()),
                gas_used: self.gas_used.into(),
                gas_limit: self.gas_limit.into(),
                logs_bloom: self.log_bloom.into(),
                timestamp: self.timestamp.into(),
                difficulty: Default::default(),
                seal_fields: vec![],
                extra_data: Default::default(),
            },
            extra_info: { BLOCK_EXTRA_INFO.clone() },
        }
    }

    pub fn rich_block(&self, include_txs: bool) -> EthRpcRichBlock {
        let eip86_transition = genesis::SPEC.params().eip86_transition;
        EthRpcRichBlock {
            inner: EthRpcBlock {
                hash: Some(self.hash.into()),
                size: None,
                // TODO: parent hash
                parent_hash: Default::default(),
                uncles_hash: Default::default(),
                author: Default::default(),
                miner: Default::default(),
                // TODO: state root
                state_root: Default::default(),
                transactions_root: Default::default(),
                receipts_root: Default::default(),
                number: Some(self.number.into()),
                gas_used: self.gas_used.into(),
                gas_limit: self.gas_limit.into(),
                logs_bloom: Some(self.log_bloom.into()),
                timestamp: self.timestamp.into(),
                difficulty: Default::default(),
                total_difficulty: None,
                seal_fields: vec![],
                uncles: vec![],
                transactions: match include_txs {
                    true => EthRpcBlockTransactions::Full(
                        self.transactions
                            .clone()
                            .into_iter()
                            .map(|txn| EthRpcTransaction::from_localized(txn, eip86_transition))
                            .collect(),
                    ),
                    false => EthRpcBlockTransactions::Hashes(
                        self.transactions
                            .clone()
                            .into_iter()
                            .map(|txn| txn.signed.hash().into())
                            .collect(),
                    ),
                },
                extra_data: Default::default(),
            },
            extra_info: BLOCK_EXTRA_INFO.clone(),
        }
    }

    pub fn add_transaction(&mut self, txn: LocalizedTransaction) {
        self.transactions.push(txn);
    }
}
