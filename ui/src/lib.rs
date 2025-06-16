//! This crate contains all shared UI for the workspace.

mod storage;
pub use storage::{StorageProvider, WalletStorage};

mod hero;
pub use hero::Hero;

mod wallet;
pub use wallet::WalletComponent;

mod peer;
