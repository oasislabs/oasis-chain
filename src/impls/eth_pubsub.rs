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

// Based on parity/rpc/src/v1/impls/eth_pubsub.rs [v1.12.0]

//! Eth PUB-SUB rpc implementation.

use std::sync::{Arc, Weak};

use ethcore::{
    filter::{Filter as EthFilter, TxEntry as EthTxEntry, TxFilter as EthTxFilter},
    ids::BlockId,
};
use failure::format_err;
use futures::{prelude::*, stream};
use jsonrpc_core::Result;
use jsonrpc_macros::{
    pubsub::{Sink, Subscriber},
    Trailing,
};
use jsonrpc_pubsub::SubscriptionId;
use log::{error, warn};
use parity_rpc::v1::{
    helpers::{errors, Subscribers},
    metadata::Metadata,
    traits::EthPubSub,
    types::{pubsub, TransactionOutcome},
};
use parking_lot::RwLock;
use tokio::spawn;

use crate::{blockchain::Blockchain, pubsub::Listener};

type PubSubClient = Sink<pubsub::Result>;

/// Eth PubSub implementation.
pub struct EthPubSubClient {
    handler: Arc<ChainNotificationHandler>,
    heads_subscribers: Arc<RwLock<Subscribers<PubSubClient>>>,
    logs_subscribers: Arc<RwLock<Subscribers<(PubSubClient, EthFilter)>>>,
    tx_subscribers: Arc<RwLock<Subscribers<(PubSubClient, EthTxFilter)>>>,
}

impl EthPubSubClient {
    /// Creates new `EthPubSubClient`.
    pub fn new(blockchain: Arc<Blockchain>) -> Self {
        let heads_subscribers = Arc::new(RwLock::new(Subscribers::default()));
        let logs_subscribers = Arc::new(RwLock::new(Subscribers::default()));
        let tx_subscribers = Arc::new(RwLock::new(Subscribers::default()));

        EthPubSubClient {
            handler: Arc::new(ChainNotificationHandler {
                blockchain,
                heads_subscribers: heads_subscribers.clone(),
                logs_subscribers: logs_subscribers.clone(),
                tx_subscribers: tx_subscribers.clone(),
            }),
            heads_subscribers,
            logs_subscribers,
            tx_subscribers,
        }
    }

    /// Returns a chain notification handler.
    pub fn handler(&self) -> Weak<ChainNotificationHandler> {
        Arc::downgrade(&self.handler)
    }
}

/// PubSub Notification handler.
pub struct ChainNotificationHandler {
    blockchain: Arc<Blockchain>,
    heads_subscribers: Arc<RwLock<Subscribers<PubSubClient>>>,
    logs_subscribers: Arc<RwLock<Subscribers<(PubSubClient, EthFilter)>>>,
    tx_subscribers: Arc<RwLock<Subscribers<(PubSubClient, EthTxFilter)>>>,
}

impl ChainNotificationHandler {
    fn notify(subscriber: &PubSubClient, result: pubsub::Result) {
        spawn(
            subscriber
                .notify(Ok(result))
                .map(|_| ())
                .map_err(move |err| warn!("Unable to send notification: {:?}", err)),
        );
    }

    fn notify_heads(&self, from_block: u64, to_block: u64) {
        // If there are no subscribers, don't do any notification processing.
        if self.heads_subscribers.read().is_empty() {
            return;
        }

        // TODO: Should we support block range fetch?
        let heads_subscribers = self.heads_subscribers.clone();
        let blockchain = self.blockchain.clone();
        spawn(
            stream::iter_ok(from_block..=to_block)
                .and_then(move |number| blockchain.get_block_by_number(number))
                .and_then(|blk| match blk {
                    Some(blk) => Ok(blk),
                    None => Err(format_err!("block not found")),
                })
                .map(|blk| blk.rich_header())
                .collect()
                .map_err(move |err| error!("Failed to fetch blocks for heads notify: {:?}", err))
                .map(move |headers| {
                    let subscribers = heads_subscribers.read();

                    for header in headers {
                        for subscriber in subscribers.values() {
                            Self::notify(subscriber, pubsub::Result::Header(header.clone()));
                        }
                    }
                }),
        );
    }

    fn notify_logs(&self, from_block: u64, to_block: u64) {
        for &(ref subscriber, ref filter) in self.logs_subscribers.read().values() {
            let mut filter = filter.clone();

            // Limit query range.
            filter.from_block = BlockId::Number(from_block);
            filter.to_block = BlockId::Number(to_block);

            let subscriber = subscriber.clone();
            spawn(
                self.blockchain
                    .logs(filter)
                    .map(move |logs| {
                        for log in logs {
                            Self::notify(&subscriber, pubsub::Result::Log(log.into()));
                        }
                    })
                    .map_err(move |err| {
                        error!("Failed to fetch logs: {:?}", err);
                    }),
            );
        }
    }
}

impl Listener for ChainNotificationHandler {
    fn notify_blocks(&self, from_block: u64, to_block: u64) {
        self.notify_heads(from_block, to_block);
        self.notify_logs(from_block, to_block);
    }

    fn notify_completed_transaction(&self, entry: &EthTxEntry, output: Vec<u8>) {
        for &(ref subscriber, ref filter) in self.tx_subscribers.read().values() {
            let filter = filter.clone();

            if !filter.matches(entry) {
                continue;
            }

            Self::notify(
                &subscriber,
                pubsub::Result::TransactionOutcome(TransactionOutcome {
                    hash: entry.transaction_hash.into(),
                    output: output.clone(),
                }),
            );
        }
    }
}

impl EthPubSub for EthPubSubClient {
    type Metadata = Metadata;

    fn subscribe(
        &self,
        _meta: Metadata,
        subscriber: Subscriber<pubsub::Result>,
        kind: pubsub::Kind,
        params: Trailing<pubsub::Params>,
    ) {
        let error = match (kind, params.into()) {
            (pubsub::Kind::NewHeads, None) => {
                self.heads_subscribers.write().push(subscriber);
                return;
            }
            (pubsub::Kind::NewHeads, _) => {
                errors::invalid_params("newHeads", "Expected no parameters.")
            }
            (pubsub::Kind::Logs, Some(pubsub::Params::Logs(filter))) => {
                self.logs_subscribers
                    .write()
                    .push(subscriber, filter.into());
                return;
            }
            (pubsub::Kind::Logs, _) => errors::invalid_params("logs", "Expected a filter object."),
            (pubsub::Kind::CompletedTransaction, Some(pubsub::Params::Transaction(filter))) => {
                self.tx_subscribers.write().push(subscriber, filter.into());
                return;
            }
            // we don't track pending transactions currently
            (pubsub::Kind::NewPendingTransactions, _) => errors::unimplemented(None),
            _ => errors::unimplemented(None),
        };

        let _ = subscriber.reject(error);
    }

    fn unsubscribe(&self, id: SubscriptionId) -> Result<bool> {
        let res = self.heads_subscribers.write().remove(&id).is_some();
        let res2 = self.logs_subscribers.write().remove(&id).is_some();
        let res3 = self.tx_subscribers.write().remove(&id).is_some();

        Ok(res || res2 || res3)
    }
}
