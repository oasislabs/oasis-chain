use std::sync::Arc;

use ekiden_keymanager::{client::MockClient, ContractId};
use ethereum_types::Address;
use futures::prelude::*;
use hash::keccak;
use jsonrpc_core::{futures::future, BoxFuture};
use jsonrpc_macros::Trailing;
use parity_rpc::v1::{
    metadata::Metadata,
    types::{BlockNumber, Bytes, H160 as RpcH160},
};

use crate::{
    blockchain::Blockchain,
    traits::oasis::{Oasis, RpcExecutionPayload, RpcPublicKeyPayload},
    util::{block_number_to_id, execution_error, jsonrpc_error},
};

/// Eth rpc implementation
pub struct OasisClient {
    blockchain: Arc<Blockchain>,
    km_client: Arc<MockClient>,
}

impl OasisClient {
    /// Creates new OasisClient.
    pub fn new(blockchain: Arc<Blockchain>, km_client: Arc<MockClient>) -> Self {
        OasisClient {
            blockchain,
            km_client,
        }
    }
}

impl Oasis for OasisClient {
    type Metadata = Metadata;

    fn public_key(&self, contract: Address) -> BoxFuture<Option<RpcPublicKeyPayload>> {
        let contract_id = ContractId::from(&keccak(contract.to_vec())[..]);

        Box::new(future::ok(self.km_client.get_public_key(contract_id).map(
            |pk_payload| RpcPublicKeyPayload {
                public_key: Bytes::from(pk_payload.key.as_ref().to_vec()),
                checksum: Bytes::from(pk_payload.checksum),
                signature: Bytes::from(pk_payload.signature.as_ref().to_vec()),
            },
        )))
    }

    fn get_expiry(&self, address: RpcH160, num: Trailing<BlockNumber>) -> BoxFuture<u64> {
        let address: Address = RpcH160::into(address);
        let num = num.unwrap_or_default();

        let state = match self.blockchain.state(block_number_to_id(num)) {
            Ok(state) => state,
            Err(err) => return Box::new(future::err(jsonrpc_error(err))),
        };

        Box::new(future::done(
            state
                .storage_expiry(&address)
                .map_err(|err| jsonrpc_error(err.into()))
                .map(Into::into),
        ))
    }

    fn invoke(&self, raw: Bytes) -> BoxFuture<RpcExecutionPayload> {
        Box::new(
            self.blockchain
                .send_raw_transaction(raw.into())
                .map_err(execution_error)
                .then(move |maybe_result| {
                    maybe_result.map(|(hash, result)| RpcExecutionPayload {
                        transaction_hash: hash.into(),
                        status_code: (result.status_code as u64).into(),
                        output: result.output.into(),
                    })
                }),
        )
    }
}
