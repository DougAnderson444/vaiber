//! Peer component once a Wallet is available.
//!
//! The logic creates a default plog if one does not exist yet.
mod controls;

use bs_peer::peer::PublicEvent;
use bs_peer::utils::create_default_scripts;
use bs_peer::{peer::DefaultBsPeer, BsPeer};
use controls::PlogControls;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use libp2p::futures::StreamExt as _;
use provenance_log::Log;

use crate::wallet::KeyMan;
use crate::StorageProvider;

const VLAD_STORAGE_KEY: &str = "VLAD_STORAGE_KEY";

#[component]
pub fn Peer(platform_content: Element) -> Element {
    let storage = use_context::<StorageProvider>();

    // use use_context_provider(|| use_signal(|| None::<InMemoryKeyManager<bs_wallets::Error>>));
    let key_manager = use_context::<Signal<Option<KeyMan>>>();
    let mut bs_peer_signal = use_signal(|| None::<DefaultBsPeer<KeyMan>>);
    let mut peer_address = use_signal(|| None::<String>);
    let mut connected_peers = use_signal(|| Vec::<String>::new());

    use_context_provider(|| connected_peers.clone());

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
            let mut peer = BsPeer::new(km).await.unwrap();

            // Try to load existing plog
            let plog_loaded = if storage.exists(VLAD_STORAGE_KEY) {
                if let Ok(plog_data) = storage.load(VLAD_STORAGE_KEY) {
                    let plog = Log::try_from(plog_data.as_slice()).unwrap();
                    peer.load(plog).await.is_ok()
                } else {
                    false
                }
            } else {
                false
            };

            // Generate new plog only if loading failed
            if !plog_loaded {
                let (lock_script, unlock_script) = create_default_scripts();
                peer.generate(&lock_script, &unlock_script)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to generate Plog: {}", e);
                    });
                // Save the new plog to storage
                if let Some(plog_data) = peer.plog().cloned() {
                    let plog_bytes: Vec<u8> = plog_data.into();
                    storage
                        .save(VLAD_STORAGE_KEY, &plog_bytes)
                        .unwrap_or_else(|e| {
                            tracing::error!("Failed to save Plog to storage: {}", e);
                        });
                }
            }

            // Spawn a task to listen for peer.events
            // and handle them accordingly
            // Move the peer.events out by taking it from the Option
            let mut peer_events = peer.events.take().unwrap();
            spawn(async move {
                while let Some(event) = peer_events.next().await {
                    match event {
                        PublicEvent::ListenAddr { address, .. } => {
                            tracing::info!("Peer listening on: {}", address);
                            peer_address.set(Some(address.to_string()));
                        }
                        PublicEvent::NewConnection { peer } => {
                            tracing::info!("New connection established with peer: {}", peer);
                            // Add the peer to our connected peers list
                            connected_peers.write().push(peer.to_string());
                        }
                        PublicEvent::ConnectionClosed { peer, cause } => {
                            tracing::info!(
                                "Connection closed with peer: {}, cause: {:?}",
                                peer,
                                cause
                            );
                            // Remove the peer from our connected peers list
                            connected_peers.write().retain(|p| p != &peer.to_string());
                        }

                        _ => {
                            tracing::debug!("Received event: {:?}", event);
                        }
                    }
                }
            });

            bs_peer_signal.set(Some(peer));
        }
    });

    // ALWAYS keep as much logic outside of the rsx! macro as possible.
    rsx! {
        div {
            class: "p-8 w-full h-full flex flex-col items-center justify-center",
            h1 { "PeerPiper vaiber" }
            // You can read resource just like a signal. If the resource is still
            // running, it will return None
            if let Some(_) = &*bs_peer_resource.read() {

                // Display the peer address if available
                if let Some(addr) = peer_address() {
                    div {
                        class: "font-semibold",
                        "Your Node Address:"
                        p {
                            class: "m-2 p-2 bg-green-50/5 border-2 border-green-500/50 rounded-lg w-full break-all",
                            "{addr}"
                        }
                    }
                }

                PlogControls { peer: bs_peer_signal }

                // Display the platform-specific content
                {platform_content}

            } else {
                "Initializing peer..."
            }
        }
    }
}
