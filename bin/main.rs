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

//! web3 gateway for Oasis Ethereum runtime.

#![deny(warnings)]

extern crate fdlimit;
extern crate signal_hook;
#[macro_use]
extern crate clap;
extern crate failure;
extern crate log;
extern crate oasis_chain;
extern crate simple_logger;

use std::{io::Read, os::unix::net::UnixStream};

use clap::{App, Arg};
use failure::Fallible;
use fdlimit::raise_fd_limit;
use log::{error, info};

use oasis_chain::{util, MIN_GAS_PRICE_GWEI};

fn main() -> Fallible<()> {
    // Increase max number of open files.
    raise_fd_limit();

    let gas_price = MIN_GAS_PRICE_GWEI.to_string();

    let args = App::new("Oasis chain")
        .arg(
            Arg::with_name("http-port")
                .long("http-port")
                .help("Port to use for JSON-RPC HTTP server.")
                .default_value("8545")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("threads")
                .long("threads")
                .help("Number of threads to use for HTTP server.")
                .default_value("1")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("ws-port")
                .long("ws-port")
                .help("Port to use for WebSocket server.")
                .default_value("8546")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("ws-max-connections")
                .long("ws-max-connections")
                .help("Max number of concurrent WebSocket connections.")
                .default_value("10000")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("pubsub-interval")
                .long("pubsub-interval")
                .help("Time interval used for pub/sub notifications (in sec).")
                .default_value("1")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("gas-price")
                .long("gas-price")
                .help("Gas price (in Gwei).")
                .default_value(&gas_price)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("interface")
                .long("interface")
                .help("Interface address for HTTP and WebSocket servers.")
                .default_value("127.0.0.1")
                .takes_value(true),
        )
        // Logging.
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .get_matches();

    let log_level = match args.occurrences_of("v") {
        0 => log::Level::Info,
        1 => log::Level::Debug,
        2 | _ => log::Level::Trace,
    };
    simple_logger::init_with_level(log_level).unwrap();

    let num_threads = value_t!(args, "threads", usize)?;
    let interface = value_t!(args, "interface", String)?;
    let http_port = value_t!(args, "http-port", u16)?;
    let ws_port = value_t!(args, "ws-port", u16)?;
    let ws_max_connections = value_t!(args, "ws-max-connections", usize)?;
    let pubsub_interval_secs = value_t!(args, "pubsub-interval", u64)?;
    let gas_price = util::gwei_to_wei(value_t!(args, "gas-price", u64)?);

    let chain_info = include_str!("../resources/info.txt");
    info!("Starting Oasis local chain\n{}", chain_info);

    let client = oasis_chain::start(
        args,
        pubsub_interval_secs,
        &interface,
        http_port,
        num_threads,
        ws_port,
        ws_max_connections,
        gas_price,
    );

    let client = match client {
        Ok(client) => client,
        Err(err) => {
            error!("Failed to initialize Oasis local chain: {:?}", err);
            return Ok(());
        }
    };

    info!("Oasis local chain is running");

    // Register a self-pipe for handing the SIGTERM and SIGINT signals.
    let (mut read, write) = UnixStream::pair()?;
    signal_hook::pipe::register(signal_hook::SIGINT, write.try_clone()?)?;
    signal_hook::pipe::register(signal_hook::SIGTERM, write.try_clone()?)?;

    // Wait for signal.
    let mut buff = [0];
    read.read_exact(&mut buff)?;

    info!("Oasis local chain is shutting down");

    client.shutdown();

    info!("Shutdown completed");

    Ok(())
}
