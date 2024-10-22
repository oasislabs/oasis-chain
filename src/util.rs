use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use ethcore::ids::BlockId;
use ethereum_types::U256;
use failure::Error;
use jsonrpc_core::{self, ErrorCode, Value};
use parity_rpc::v1::types::BlockNumber;

pub fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn gwei_to_wei(gwei: u64) -> U256 {
    U256::from(gwei).saturating_mul(U256::from(1_000_000_000))
}

/// Convert an RPC block number to block id.
pub fn block_number_to_id(number: BlockNumber) -> BlockId {
    // For "pending", just use latest block.
    match number {
        BlockNumber::Num(num) => BlockId::Number(num),
        BlockNumber::Earliest => BlockId::Earliest,
        BlockNumber::Latest => BlockId::Latest,
        BlockNumber::Pending => BlockId::Latest,
    }
}

/// Constructs a JSON-RPC error from a string message, with error code -32603.
pub fn jsonrpc_error(err: Error) -> jsonrpc_core::Error {
    jsonrpc_core::Error {
        code: ErrorCode::InternalError,
        message: format!("{}", err),
        data: None,
    }
}

/// Constructs a JSON-RPC error for a transaction execution error.
/// TODO: format error message
pub fn execution_error<T: fmt::Display>(data: T) -> jsonrpc_core::Error {
    jsonrpc_core::Error {
        code: ErrorCode::ServerError(-32015),
        message: format!("Transaction execution error with cause: {}", data),
        data: Some(Value::String(format!("{}", data))),
    }
}
