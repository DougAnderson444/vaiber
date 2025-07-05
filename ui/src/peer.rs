//! Peer component once a Wallet is available.
//!
//! The logic creates a default plog if one does not exist yet.
mod controls;

use std::collections::HashMap;
use std::path::PathBuf;

use bs::params::anykey::PubkeyParams;
use bs::update::OpParams;
use bs_peer::peer::{Libp2pEvent, PublicEvent, ResolvedPlog, ResolverExt as _};
use bs_peer::platform::StartConfig;
use bs_peer::utils::create_default_scripts;
use bs_peer::{peer::DefaultBsPeer, BsPeer};
use controls::PlogControls;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use libp2p::futures::StreamExt as _;
use multicid::Vlad;
use provenance_log::key::key_paths::ValidatedKeyParams as _;
use provenance_log::resolver::Resolver;
use provenance_log::{Key, Log, Script};

use crate::wallet::KeyMan;
use crate::StorageProvider;

const VLAD_STORAGE_KEY: &str = "VLAD_STORAGE_KEY";

#[component]
pub fn Peer(platform_content: Element, base_path: Option<PathBuf>) -> Element {
    let storage = use_context::<StorageProvider>();

    let (lock_script, unlock_script) = create_default_scripts();

    // use use_context_provider(|| use_signal(|| None::<InMemoryKeyManager<bs_wallets::Error>>));
    let key_manager = use_context::<Signal<Option<KeyMan>>>();
    let mut bs_peer_signal = use_signal(|| None::<DefaultBsPeer<KeyMan>>);
    let mut plog_signal = use_signal(|| None::<Log>);
    let mut peer_address = use_signal(|| None::<String>);
    let mut connected_peers = use_signal(Vec::<String>::new);

    // List of connected peers's Vlad to Plogs mappings
    let mut peer_list = use_signal(HashMap::<Vlad, Option<ResolvedPlog>>::new);

    // Provide Peer List and Connected Peers to child components
    use_context_provider(|| peer_list);
    use_context_provider(|| connected_peers);
    use_context_provider(|| plog_signal);

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

    let unlock_script_clone = unlock_script.clone();
    let storage_clone = storage.clone();
    let bs_peer_resource = use_resource(move || {
        let km = km.clone();
        let storage = storage_clone.clone();
        let lock_clone = lock_script.clone();
        let unlock_clone = unlock_script.clone();
        let bath_path_clone = base_path.clone();
        async move {
            let mut peer = BsPeer::new(
                km,
                StartConfig {
                    base_path: bath_path_clone,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

            // Try to load existing plog
            let plog_loaded = if storage.exists(VLAD_STORAGE_KEY) {
                tracing::info!("Loading existing Plog from storage...");
                if let Ok(plog_data) = storage.load(VLAD_STORAGE_KEY) {
                    tracing::info!("Plog loaded from storage successfully.");
                    let plog = Log::try_from(plog_data.as_slice()).unwrap();
                    peer.load(plog).await.is_ok()
                } else {
                    false
                }
            } else {
                false
            };

            tracing::info!("Plog loaded from storage: {}", plog_loaded);

            // Generate new plog only if loading failed
            if !plog_loaded {
                peer.generate(&lock_clone, &unlock_clone.clone())
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to generate Plog: {}", e);
                    });
                // Save the new plog to storage
                if let Some(plog_data) = peer.plog() {
                    let plog_bytes: Vec<u8> = plog_data.into();
                    storage
                        .save(VLAD_STORAGE_KEY, &plog_bytes)
                        .unwrap_or_else(|e| {
                            tracing::error!("Failed to save Plog to storage: {}", e);
                        });
                };
            }

            // set the plog_signal with the plog from the peer
            if let Some(plog) = peer.plog() {
                plog_signal.set(Some(plog.clone()));
            } else {
                tracing::error!("Plog is not initialized.");
            }

            // We can't call async methods on a Signals directly,
            // sicne the read would be held across await points,
            // so we need to use a closure instead.
            let peer_clone = peer.clone();
            let update_dht = move || {
                // Need to clone each time, becuase FnOnce consumes each time
                let peer_clone_inner = peer_clone.clone();
                // This function will be called to update the DHT with the plog
                async move {
                    if let Err(e) = peer_clone_inner.record_plog_to_dht().await {
                        tracing::error!("Failed to publish records: {}", e);
                        return;
                    }
                    tracing::info!("Plog records published to DHT successfully.");
                }
            };

            // let peer_clone = peer.clone();
            // let update_plog_closure = move |additional_ops: Vec<OpParams>| {
            //     // Need to clone each time, becuase FnOnce consumes each time
            //     let mut peer_clone_inner = peer_clone.clone();
            //     let unlock_script_clone = unlock_clone.clone();
            //     // This function will be called to update the DHT with the plog
            //     async move {
            //         let update_cfg = bs::update::Config::builder()
            //             .unlock(Script::Code(Key::default(), unlock_script_clone))
            //             .entry_signing_key(PubkeyParams::KEY_PATH.into())
            //             .additional_ops(additional_ops)
            //             .build();
            //         if let Err(e) = peer_clone_inner.update(update_cfg).await {
            //             tracing::error!("Failed to publish records: {}", e);
            //             return;
            //         }
            //         tracing::info!("Plog records published to DHT successfully.");
            //     };
            // };

            // Spawn a task to listen for peer.events
            // and handle them accordingly
            // Move the peer.events out by taking it from the Option
            let mut peer_events = peer.events.take().unwrap();
            let peer_clone = peer.clone();
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

                            // When we get a new connection, we should re-publish records on the
                            // DHT so that the newly connected peer gets DHT updates pronto.
                            update_dht().await;
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
                        PublicEvent::Message { topic, data, .. } => {
                            tracing::info!("Received message topic: {}, data: {:?}", topic, data);
                            // If the topic is a Vlad, then the data should be the plog head bytes,
                            // from which we can resolve a plog witht he Resolver.
                            if let Ok(vlad) = Vlad::try_from_str(&topic) {
                                tracing::info!("Received Vlad Message: {}", vlad);
                                // Resolve the plog head CID bytes to a plog
                                if let Some(network_client) = peer_clone.network_client.as_ref() {
                                    let Ok(head) = multicid::Cid::try_from(data.as_slice()) else {
                                        tracing::warn!("Invalid VLAD: {}", vlad);
                                        return;
                                    };
                                    if let Ok(plog) = network_client.resolve_plog(&head).await {
                                        tracing::info!("Resolved plog: {:?}", plog);
                                        // Add Plog to list of Vlads
                                        peer_list.write().insert(vlad, Some(plog));
                                    } else {
                                        tracing::error!("Failed to resolve plog head CID bytes.");
                                    }
                                } else {
                                    tracing::error!("Network client is not available.");
                                }
                            }
                        }
                        PublicEvent::Swarm(Libp2pEvent::PutRecordRequest { source }) => {
                            tracing::info!("Received PutRecordRequest from: {}", source);
                            // use network_client to look up all Vlads in the peer_list
                            // and update their Plog values with the values from the DHT.
                            if let Some(network_client) = peer_clone.network_client.as_ref() {
                                let peer_list_clone = peer_list.read().clone();
                                for (vlad, _plog) in peer_list_clone {
                                    tracing::info!("Checking for Vlad: {}", vlad);

                                    #[cfg(not(target_arch = "wasm32"))]
                                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;

                                    tracing::info!("Resolving Vlad: {}", vlad);

                                    let vlad_bytes: Vec<u8> = vlad.clone().into();
                                    let Ok(cid_bytes) = network_client.get_record(vlad_bytes).await
                                    else {
                                        tracing::warn!("Failed to get record for Vlad: {}", vlad);
                                        continue;
                                    };

                                    let Ok(head) = multicid::Cid::try_from(cid_bytes.as_slice())
                                    else {
                                        tracing::warn!("Invalid CID bytes for Vlad: {}", vlad);
                                        continue;
                                    };

                                    tracing::info!(
                                        "Resolved head CID for Vlad {}: {:?}",
                                        vlad,
                                        head
                                    );

                                    let Ok(head_bytes) = network_client.resolve(&head).await else {
                                        tracing::warn!(
                                            "Failed to resolve head CID {} for Vlad: {}",
                                            head,
                                            vlad
                                        );
                                        continue;
                                    };

                                    tracing::info!(
                                        "Resolved head bytes for Vlad {}: {:?}",
                                        vlad,
                                        head_bytes
                                    );

                                    let vlad_clone = vlad.clone();
                                    let network_client_clone = network_client.clone();
                                    spawn(async move {
                                        if let Ok(resolved_plog) =
                                            network_client_clone.resolve_plog(&head).await
                                        {
                                            tracing::info!(
                                                "Resolved Plog for Vlad {}: {:?}",
                                                vlad_clone,
                                                resolved_plog
                                            );
                                            // Update the peer_list with the resolved plog
                                            peer_list
                                                .write()
                                                .insert(vlad_clone, Some(resolved_plog));
                                        } else {
                                            tracing::error!(
                                                "Failed to resolve Plog for Vlad {}",
                                                vlad_clone
                                            );
                                        }
                                    });
                                }
                            } else {
                                tracing::error!("Network client is not available.");
                            }
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

                // Allow user to add additonal_ops to the update::Config and ad to plog
    // Shows a simple form to add key vlaue pairs which get built into OpParms
                div {
                    class: "w-full max-w-md mt-4",
                    h2 { "Add Additional Operations" }
                    form {
                        onsubmit: move |e| {
                            let mut additional_ops = vec![];
                            // Here you would collect the key-value pairs from the form inputs
                            // and build OpParams to add to the plog.
                            // For simplicity, we will just create a dummy OpParams.
                            let k = e.data.values().get("key").unwrap().clone().to_vec();
                            let s = e.data.values().get("value").unwrap().clone().to_vec();
                            additional_ops.push(OpParams::UseStr {
                                key: Key::try_from(k[0].clone()).unwrap_or_default(),
                                s: s[0].clone(),
                            });

                            let update_cfg = bs::update::Config::builder()
                                .unlock(Script::Code(Key::default(), unlock_script_clone.clone()))
                                .entry_signing_key(PubkeyParams::KEY_PATH.into())
                                .additional_ops(additional_ops)
                                .build();

                            if let Some(peer) = bs_peer_signal.read().as_ref() {
                                // Call the update method on the peer with the new configuration
                                let mut peer_clone = peer.clone();
                                let storage = storage.clone();
                                spawn(async move {
                                    if let Err(e) = peer_clone.update(update_cfg).await {
                                        tracing::error!("Failed to update plog: {}", e);
                                    } else {
                                        tracing::info!("Plog updated successfully.");
                                    }

                                    if let Some(ref plog) = peer_clone.plog() {
                                        // Save the updated plog to storage
                                        let plog_bytes: Vec<u8> = plog.clone().into();
                                        if let Err(e) = storage.save(VLAD_STORAGE_KEY, &plog_bytes) {
                                            tracing::error!("Failed to save Plog to storage: {}", e);
                                        } else {
                                            tracing::info!("Plog saved to storage successfully.");

                                            // update plog signal with this plog value
                                            plog_signal.set(Some(plog.clone()));
                                        }
                                    } else {
                                        tracing::error!("Plog is not initialized.");
                                    }
                                });
                            } else {
                                tracing::error!("Peer is not initialized.");
                            }
                        },
                        KeyInput { }
                        input {
                            type: "text",
                            placeholder: "Value",
                            name: "value",
                        }
                        button {
                            type: "submit",
                            "Add Operation"
                        }
                    }
                }

                // Display the platform-specific content
                {platform_content}

            } else {
                "Initializing peer..."
            }
        }
    }
}

/// Input component which validates the [Key] input
#[component]
fn KeyInput() -> Element {
    let mut key = use_signal(String::new);
    let error = use_signal(|| None::<String>);

    rsx! {
        div {
            class: "w-full max-w-md mt-4",
            input {
                type: "text",
                placeholder: "Enter Key",
                value: "{key}",
                name: "key",
                oninput: move |e| {
                    // add leading "/" if absent,
                    // replace any spaces or non-alphanumeric characters with "/"
                    let mut input = e.value().clone();
                    if !input.starts_with('/') {
                        input.insert(0, '/');
                    }
                    input = input.replace(" ", "/");
                    input = input.replace(|c: char| !c.is_alphanumeric() && c != '/', "/");
                    key.set(input.clone());
                }
            }
            if let Some(err) = error() {
                p { class: "text-red-500", "{err}" }
            }
        }
    }
}
