// Copyright 2015-2018 Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

//! Eth rpc implementation.

use std::sync::Arc;

use ethcore::{filter::Filter as EthcoreFilter, ids::BlockId};
use ethereum_types::{Address, H256, U256};
use failure::Error;
use jsonrpc_core::{
    futures::{future, Future},
    BoxFuture, Result,
};
use jsonrpc_macros::Trailing;
use parity_rpc::v1::{
    helpers::{errors, fake_sign},
    metadata::Metadata,
    traits::Eth,
    types::{
        BlockNumber, Bytes, CallRequest, Filter, Index, Log as RpcLog, Receipt as RpcReceipt,
        RichBlock, Transaction as RpcTransaction, Work, H160 as RpcH160, H256 as RpcH256,
        H64 as RpcH64, U256 as RpcU256,
    },
};

use crate::{
    blockchain::Blockchain,
    genesis,
    util::{block_number_to_id, execution_error, jsonrpc_error},
};

// short for "try_boxfuture"
// unwrap a result, returning a BoxFuture<_, Err> on failure.
macro_rules! try_bf {
    ($res:expr) => {
        match $res {
            Ok(val) => val,
            Err(e) => return Box::new(::jsonrpc_core::futures::future::err(e.into())),
        }
    };
}

/// Eth rpc implementation.
pub struct EthClient {
    blockchain: Arc<Blockchain>,
}

#[derive(Debug)]
enum BlockNumberOrId {
    Number(BlockNumber),
    Id(BlockId),
}

impl From<BlockId> for BlockNumberOrId {
    fn from(value: BlockId) -> BlockNumberOrId {
        BlockNumberOrId::Id(value)
    }
}

impl From<BlockNumber> for BlockNumberOrId {
    fn from(value: BlockNumber) -> BlockNumberOrId {
        BlockNumberOrId::Number(value)
    }
}

impl EthClient {
    /// Creates new EthClient.
    pub fn new(blockchain: Arc<Blockchain>) -> Self {
        EthClient { blockchain }
    }
}

impl Eth for EthClient {
    type Metadata = Metadata;

    fn protocol_version(&self) -> Result<String> {
        // Ethereum wire protocol version: https://github.com/ethereum/wiki/wiki/Ethereum-Wire-Protocol#fast-synchronization-pv63
        Ok(format!("{}", 63))
    }

    fn syncing(&self) -> Result<bool> {
        Ok(false)
    }

    fn author(&self, _meta: Metadata) -> Result<RpcH160> {
        Ok(Default::default())
    }

    fn is_mining(&self) -> Result<bool> {
        Ok(true)
    }

    fn hashrate(&self) -> Result<RpcU256> {
        Ok(RpcU256::from(0))
    }

    fn gas_price(&self) -> Result<RpcU256> {
        Ok(self.blockchain.gas_price().into())
    }

    fn accounts(&self, _meta: Metadata) -> Result<Vec<RpcH160>> {
        Ok(vec![])
    }

    fn block_number(&self) -> BoxFuture<RpcU256> {
        Box::new(
            self.blockchain
                .get_latest_block()
                .map(|blk| RpcU256::from(blk.number()))
                .map_err(jsonrpc_error),
        )
    }

    fn balance(&self, address: RpcH160, num: Trailing<BlockNumber>) -> BoxFuture<RpcU256> {
        let address = address.into();
        let num = num.unwrap_or_default();

        let state = match self.blockchain.state(block_number_to_id(num)) {
            Ok(state) => state,
            Err(err) => return Box::new(future::err(jsonrpc_error(err))),
        };

        Box::new(future::done(
            state
                .balance(&address)
                .map_err(|err| jsonrpc_error(err.into()))
                .map(Into::into),
        ))
    }

    fn storage_at(
        &self,
        address: RpcH160,
        pos: RpcU256,
        num: Trailing<BlockNumber>,
    ) -> BoxFuture<RpcH256> {
        let address = address.into();
        let pos: U256 = RpcU256::into(pos);
        let num = num.unwrap_or_default();

        let state = match self.blockchain.state(block_number_to_id(num)) {
            Ok(state) => state,
            Err(err) => return Box::new(future::err(jsonrpc_error(err))),
        };

        Box::new(future::done(
            state
                .storage_at(&address, &pos.into())
                .map_err(|err| jsonrpc_error(err.into()))
                .map(Into::into),
        ))
    }

    fn transaction_count(
        &self,
        address: RpcH160,
        num: Trailing<BlockNumber>,
    ) -> BoxFuture<RpcU256> {
        let address: Address = RpcH160::into(address);
        let num = num.unwrap_or_default();

        let state = match self.blockchain.state(block_number_to_id(num)) {
            Ok(state) => state,
            Err(err) => return Box::new(future::err(jsonrpc_error(err))),
        };

        Box::new(future::done(
            state
                .nonce(&address)
                .map_err(|err| jsonrpc_error(err.into()))
                .map(Into::into),
        ))
    }

    fn block_transaction_count_by_hash(&self, hash: RpcH256) -> BoxFuture<Option<RpcU256>> {
        Box::new(
            self.blockchain
                .get_block_by_hash(hash.into())
                .and_then(|blk| future::ok(blk.map(|blk| blk.transactions().len().into())))
                .map_err(jsonrpc_error),
        )
    }

    fn block_transaction_count_by_number(&self, num: BlockNumber) -> BoxFuture<Option<RpcU256>> {
        // We don't track pending transactions.
        if let BlockNumber::Pending = num {
            return Box::new(future::ok(Some(0.into())));
        }

        Box::new(
            self.blockchain
                .get_block(block_number_to_id(num))
                .and_then(|blk| future::ok(blk.map(|blk| blk.transactions().len().into())))
                .map_err(jsonrpc_error),
        )
    }

    fn block_uncles_count_by_hash(&self, _hash: RpcH256) -> BoxFuture<Option<RpcU256>> {
        // We do not have uncles.
        Box::new(future::ok(None))
    }

    fn block_uncles_count_by_number(&self, _num: BlockNumber) -> BoxFuture<Option<RpcU256>> {
        // We do not have uncles.
        Box::new(future::ok(None))
    }

    fn code_at(&self, address: RpcH160, num: Trailing<BlockNumber>) -> BoxFuture<Bytes> {
        let address: Address = RpcH160::into(address);
        let num = num.unwrap_or_default();

        let state = match self.blockchain.state(block_number_to_id(num)) {
            Ok(state) => state,
            Err(err) => return Box::new(future::err(jsonrpc_error(err))),
        };

        Box::new(future::done(
            state
                .code(&address)
                .map_err(|err| jsonrpc_error(err.into()))
                .map(|code| code.map_or_else(Bytes::default, |b| Bytes::new((&*b).clone()))),
        ))
    }

    fn block_by_hash(&self, hash: RpcH256, include_txs: bool) -> BoxFuture<Option<RichBlock>> {
        Box::new(
            self.blockchain
                .get_block_by_hash(hash.into())
                .and_then(
                    move |blk| -> Box<dyn Future<Item = _, Error = Error> + Send> {
                        match blk {
                            Some(blk) => Box::new(future::ok(Some(blk.rich_block(include_txs)))),
                            None => Box::new(future::ok(None)),
                        }
                    },
                )
                .map_err(jsonrpc_error),
        )
    }

    fn block_by_number(&self, num: BlockNumber, include_txs: bool) -> BoxFuture<Option<RichBlock>> {
        Box::new(
            self.blockchain
                .get_block(block_number_to_id(num))
                .and_then(
                    move |blk| -> Box<dyn Future<Item = _, Error = Error> + Send> {
                        match blk {
                            Some(blk) => Box::new(future::ok(Some(blk.rich_block(include_txs)))),
                            None => Box::new(future::ok(None)),
                        }
                    },
                )
                .map_err(jsonrpc_error),
        )
    }

    fn transaction_by_hash(&self, hash: RpcH256) -> BoxFuture<Option<RpcTransaction>> {
        let hash = hash.into();
        let eip86_transition = genesis::SPEC.params().eip86_transition;

        Box::new(
            self.blockchain
                .get_txn_by_hash(hash)
                .and_then(move |txn| {
                    txn.map(|txn| Ok(RpcTransaction::from_localized(txn, eip86_transition)))
                        .transpose()
                })
                .map_err(jsonrpc_error),
        )
    }

    fn transaction_by_block_hash_and_index(
        &self,
        hash: RpcH256,
        index: Index,
    ) -> BoxFuture<Option<RpcTransaction>> {
        let hash = hash.into();
        let eip86_transition = genesis::SPEC.params().eip86_transition;

        Box::new(
            self.blockchain
                .get_txn_by_block_hash_and_index(hash, index.value() as u32)
                .and_then(move |txn| {
                    txn.map(|txn| Ok(RpcTransaction::from_localized(txn, eip86_transition)))
                        .transpose()
                })
                .map_err(jsonrpc_error),
        )
    }

    fn transaction_by_block_number_and_index(
        &self,
        num: BlockNumber,
        index: Index,
    ) -> BoxFuture<Option<RpcTransaction>> {
        // We don't have pending transactions.
        if let BlockNumber::Pending = num {
            return Box::new(future::ok(None));
        }

        let eip86_transition = genesis::SPEC.params().eip86_transition;

        Box::new(
            self.blockchain
                .get_txn(block_number_to_id(num), index.value() as u32)
                .and_then(move |txn| {
                    txn.map(|txn| Ok(RpcTransaction::from_localized(txn, eip86_transition)))
                        .transpose()
                })
                .map_err(jsonrpc_error),
        )
    }

    fn transaction_receipt(&self, hash: RpcH256) -> BoxFuture<Option<RpcReceipt>> {
        let hash: H256 = hash.into();
        Box::new(
            self.blockchain
                .get_txn_receipt_by_hash(hash)
                .map_err(jsonrpc_error)
                .map(|receipt| receipt.map(Into::into)),
        )
    }

    fn uncle_by_block_hash_and_index(
        &self,
        _hash: RpcH256,
        _index: Index,
    ) -> BoxFuture<Option<RichBlock>> {
        // We do not have uncles.
        Box::new(future::ok(None))
    }

    fn uncle_by_block_number_and_index(
        &self,
        _num: BlockNumber,
        _index: Index,
    ) -> BoxFuture<Option<RichBlock>> {
        // We do not have uncles.
        Box::new(future::ok(None))
    }

    fn compilers(&self) -> Result<Vec<String>> {
        Err(errors::deprecated(
            "Compilation functionality is deprecated.".to_string(),
        ))
    }

    fn logs(&self, filter: Filter) -> BoxFuture<Vec<RpcLog>> {
        let filter: EthcoreFilter = filter.into();

        Box::new(
            self.blockchain
                .clone()
                .logs(filter)
                .map_err(jsonrpc_error)
                .map(|logs| logs.into_iter().map(Into::into).collect()),
        )
    }

    fn work(&self, _no_new_work_timeout: Trailing<u64>) -> Result<Work> {
        Err(errors::unimplemented(None))
    }

    fn submit_work(&self, _nonce: RpcH64, _pow_hash: RpcH256, _mix_hash: RpcH256) -> Result<bool> {
        Err(errors::unimplemented(None))
    }

    fn submit_hashrate(&self, _rate: RpcU256, _id: RpcH256) -> Result<bool> {
        Err(errors::unimplemented(None))
    }

    fn send_raw_transaction(&self, raw: Bytes) -> BoxFuture<RpcH256> {
        Box::new(
            self.blockchain
                .send_raw_transaction(raw.into())
                .map(|(hash, _result)| hash.into())
                .map_err(execution_error),
        )
    }

    fn submit_transaction(&self, raw: Bytes) -> BoxFuture<RpcH256> {
        self.send_raw_transaction(raw)
    }

    fn call(
        &self,
        meta: Self::Metadata,
        request: CallRequest,
        num: Trailing<BlockNumber>,
    ) -> BoxFuture<Bytes> {
        let num = num.unwrap_or_default();

        let signed = try_bf!(fake_sign::sign_call(request.into(), meta.is_dapp()));

        Box::new(
            self.blockchain
                .simulate_transaction(signed, block_number_to_id(num))
                .map_err(errors::call)
                .and_then(|executed| match executed.exception {
                    Some(ref exception) => Err(errors::vm(exception, &executed.output)),
                    None => Ok(executed),
                })
                .map(|executed| executed.output.into()),
        )
    }

    fn estimate_gas(
        &self,
        meta: Self::Metadata,
        request: CallRequest,
        num: Trailing<BlockNumber>,
    ) -> BoxFuture<RpcU256> {
        let num = num.unwrap_or_default();

        let signed = try_bf!(fake_sign::sign_call(request.into(), meta.is_dapp()));

        Box::new(
            self.blockchain
                .estimate_gas(signed, block_number_to_id(num))
                .map_err(execution_error)
                .map(Into::into),
        )
    }

    fn compile_lll(&self, _: String) -> Result<Bytes> {
        Err(errors::deprecated(
            "Compilation of LLL via RPC is deprecated".to_string(),
        ))
    }

    fn compile_serpent(&self, _: String) -> Result<Bytes> {
        Err(errors::deprecated(
            "Compilation of Serpent via RPC is deprecated".to_string(),
        ))
    }

    fn compile_solidity(&self, _: String) -> Result<Bytes> {
        Err(errors::deprecated(
            "Compilation of Solidity via RPC is deprecated".to_string(),
        ))
    }
}
