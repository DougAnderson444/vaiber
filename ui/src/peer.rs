//! Peer component once a Wallet is available.
//!
//! The logic creates a default plog if one does not exist yet.
mod controls;

use bs_peer::utils::create_default_scripts;
use bs_peer::{peer::DefaultBsPeer, BsPeer};
use controls::PlogControls;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use provenance_log::Log;

use crate::wallet::KeyMan;
use crate::StorageProvider;

const VLAD_STORAGE_KEY: &str = "vlad_storage_key";

#[component]
pub fn Peer() -> Element {
    let storage = use_context::<StorageProvider>();

    // use use_context_provider(|| use_signal(|| None::<InMemoryKeyManager<bs_wallets::Error>>));
    let key_manager = use_context::<Signal<Option<KeyMan>>>();
    let mut bs_peer_signal = use_signal(|| None::<DefaultBsPeer<KeyMan>>);

    // assert that key manager is available, return a warning if not
    if key_manager().is_none() {
        return rsx! {
            div {
                class: "w-full h-full flex items-center justify-center",
                p { "No Key Manager available. Please initialize it first." }
            }
        };
    }

    let km = key_manager.read().clone().unwrap();

    let bs_peer_resource = use_resource(move || {
        let km = km.clone();
        let storage = storage.clone();
        async move {
            let mut p = BsPeer::new(km).await.unwrap();

            // Try to load existing plog
            let plog_loaded = if storage.exists(VLAD_STORAGE_KEY) {
                if let Ok(plog_data) = storage.load(VLAD_STORAGE_KEY) {
                    let plog = Log::try_from(plog_data.as_slice()).unwrap();
                    p.load(plog).await.is_ok()
                } else {
                    false
                }
            } else {
                false
            };

            // Generate new plog only if loading failed
            if !plog_loaded {
                let (lock_script, unlock_script) = create_default_scripts();
                p.generate(&lock_script, &unlock_script)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to generate Plog: {}", e);
                    });
                // Save the new plog to storage
                if let Some(plog_data) = p.plog().cloned() {
                    let plog_bytes: Vec<u8> = plog_data.into();
                    storage
                        .save(VLAD_STORAGE_KEY, &plog_bytes)
                        .unwrap_or_else(|e| {
                            tracing::error!("Failed to save Plog to storage: {}", e);
                        });
                }
            }

            bs_peer_signal.set(Some(p));
        }
    });

    // ALWAYS keep as much logic outside of the rsx! macro as possible.
    rsx! {
        div {
            class: "w-full h-full flex flex-col items-center justify-center",
            h1 { "PeerPiper vaiber" }
            // You can read resource just like a signal. If the resource is still
            // running, it will return None
            if let Some(_) = &*bs_peer_resource.read() {
                p { "Peer initialized successfully!" }

                PlogControls { peer: bs_peer_signal }

            } else {
                "Initializing peer..."
            }
        }
    }
}
