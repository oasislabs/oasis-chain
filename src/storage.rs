//! Storage wrappers.
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use ethcore::mkvs::MKVS;

/// In-memory trivial key/value storage.
#[derive(Clone)]
pub struct MemoryMKVS(Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>);

impl MemoryMKVS {
    pub fn new() -> Self {
        MemoryMKVS(Arc::new(RwLock::new(HashMap::new())))
    }
}

impl MKVS for MemoryMKVS {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.0.read().unwrap().get(key).map(|v| v.clone())
    }

    fn insert(&mut self, key: &[u8], value: &[u8]) -> Option<Vec<u8>> {
        self.0
            .write()
            .unwrap()
            .insert(key.to_vec(), value.to_vec())
            .map(|v| v.clone())
    }

    fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.0.write().unwrap().remove(key).map(|v| v.clone())
    }

    fn boxed_clone(&self) -> Box<dyn MKVS> {
        Box::new(self.clone())
    }
}
