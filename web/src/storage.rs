use gloo_storage::{LocalStorage, Storage};
use std::sync::Mutex;
use ui::storage::WalletStorage;
use wasm_bindgen::JsValue;
use web_sys::console;

#[derive(Clone)]
pub struct WebStorage;
