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

//! Oasis local chain.

extern crate clap;
extern crate futures;
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate parking_lot;
#[macro_use]
extern crate serde_derive;
extern crate jsonrpc_core;
#[macro_use]
extern crate jsonrpc_macros;
extern crate ethcore;
extern crate ethereum_types;
extern crate failure;
extern crate hashdb;
extern crate jsonrpc_http_server;
extern crate jsonrpc_pubsub;
extern crate jsonrpc_ws_server;
extern crate keccak_hash as hash;
extern crate parity_reactor;
extern crate parity_rpc;
extern crate tokio;
extern crate tokio_threadpool;
extern crate zeroize;

extern crate ekiden_crypto;
extern crate ekiden_keymanager;

mod blockchain;
mod confidential;
mod genesis;
mod impls;
mod informant;
mod middleware;
mod parity;
mod pubsub;
mod rpc;
mod rpc_apis;
mod run;
mod servers;
mod storage;
mod traits;
pub mod util;

use std::sync::Arc;

use clap::ArgMatches;
use ethereum_types::U256;
use failure::Fallible;

use ekiden_keymanager::client::MockClient;

pub use self::{
    blockchain::{BLOCK_GAS_LIMIT, MIN_GAS_PRICE_GWEI},
    run::RunningGateway,
};

pub fn start(
    _args: ArgMatches,
    pubsub_interval_secs: u64,
    interface: &str,
    http_port: u16,
    num_threads: usize,
    ws_port: u16,
    ws_max_connections: usize,
    gas_price: U256,
    block_gas_limit: U256,
) -> Fallible<RunningGateway> {
    let km_client = Arc::new(MockClient::new());

    run::execute(
        km_client,
        pubsub_interval_secs,
        interface,
        http_port,
        num_threads,
        ws_port,
        ws_max_connections,
        gas_price,
        block_gas_limit,
    )
}
