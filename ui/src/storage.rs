//! This module defines the `WalletStorage` trait for managing wallet data.
use dioxus::prelude::*;
use std::sync::Arc;

pub trait WalletStorage: Send + Sync {
    fn save(&self, key: &str, data: &[u8]) -> Result<(), String>;
    fn load(&self, key: &str) -> Result<Vec<u8>, String>;
    fn delete(&self, key: &str) -> Result<(), String>;
    fn exists(&self, key: &str) -> bool;
}

// A storage provider context that wraps any storage implementation
#[derive(Clone)]
pub struct StorageProvider {
    inner: Arc<dyn WalletStorage>,
}

impl StorageProvider {
    pub fn new<S: WalletStorage + 'static>(storage: S) -> Self {
        Self {
            inner: Arc::new(storage),
        }
    }

    pub fn save(&self, key: &str, data: &[u8]) -> Result<(), String> {
        self.inner.save(key, data)
    }

    pub fn load(&self, key: &str) -> Result<Vec<u8>, String> {
        self.inner.load(key)
    }

    pub fn delete(&self, key: &str) -> Result<(), String> {
        self.inner.delete(key)
    }

    pub fn exists(&self, key: &str) -> bool {
        self.inner.exists(key)
    }
}

// Create a Dioxus context for the storage provider
