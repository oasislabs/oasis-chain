//! Mock key manager client which stores everything locally.

use std::{collections::HashMap, sync::Mutex};

use ekiden_crypto::signature::Signature;

use crate::api::{ContractId, ContractKey, SignedPublicKey};

/// Mock key manager client which stores everything locally.
pub struct MockClient {
    keys: Mutex<HashMap<ContractId, ContractKey>>,
}

impl MockClient {
    /// Create a new mock key manager client.
    pub fn new() -> Self {
        Self {
            keys: Mutex::new(HashMap::new()),
        }
    }
}

impl MockClient {
    pub fn get_or_create_keys(&self, contract_id: ContractId) -> ContractKey {
        let mut keys = self.keys.lock().unwrap();
        match keys.get(&contract_id) {
            Some(key) => key.clone(),
            None => {
                let key = ContractKey::generate_mock();
                keys.insert(contract_id, key.clone());
                key
            }
        }
    }

    pub fn get_public_key(&self, contract_id: ContractId) -> Option<SignedPublicKey> {
        Some(SignedPublicKey {
            key: self.get_or_create_keys(contract_id).input_keypair.get_pk(),
            checksum: vec![],
            signature: Signature::default(),
        })
    }
}
