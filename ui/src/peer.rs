//! Peer component once a Wallet is available.
//!
//! The logic creates a default plog if one does not exist yet.

use crate::wallet::KeyMan;
use crate::StorageProvider;
use bs::params::anykey::PubkeyParams;
use bs::update::OpParams;
use bs_peer::peer::{DefaultBsPeer, Libp2pEvent, PublicEvent, ResolvedPlog, ResolverExt as _};
use bs_peer::platform::StartConfig;
use bs_peer::utils::create_default_scripts;
use bs_peer::BsPeer;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use libp2p::futures::StreamExt as _;
use libp2p::Multiaddr;
use multicid::Vlad;
use provenance_log::key::key_paths::ValidatedKeyParams as _;
use provenance_log::resolver::Resolver;
use provenance_log::{Key, Log, Script};
use std::collections::HashMap;
use std::path::PathBuf;

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

                PlogControls { peer: bs_peer_signal }

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

#[component]
pub fn PlogControls(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    let plog_signal = use_context::<Signal<Option<provenance_log::Log>>>();
    // should show peer.plog.vlad as string for FYI.
    // let has_plog = move || {
    //     peer.read()
    //         .as_ref()
    //         .map(|p| p.plog().is_some())
    //         .unwrap_or(false)
    // };

    // let vlad_string = move || {
    //     peer.read()
    //         .as_ref()
    //         .and_then(|p| {
    //             p.plog()
    //                 .lock_async()
    //                 .await
    //                 .unwrap()
    //                 .as_ref()
    //                 .map(|plog| plog.vlad.to_string())
    //         })
    //         .unwrap_or_else(|| "No Plog available".to_string())
    // };

    let peer_clone = peer;
    let vlad_resource = use_resource(move || {
        let peer = peer_clone;
        async move {
            let binding = peer.read().as_ref().unwrap().plog();
            let Some(ref plog) = binding else {
                return "No Plog available".to_string();
            };

            plog.vlad.to_string()
        }
    });

    rsx! {
        div {
            class: "w-full h-full flex flex-col items-center justify-center",
            h1 { "Plog Details" }
            p { "This is you Verifiable Long-Lived Address (VLAD). It will stay the same, even when you rotate keys. No blockchain required, it's all peer to peer." }
            div {
                class: "m-2 p-2 bg-green-50/5 border-2 border-green-500/50 rounded-lg w-full break-all",
                span {
                    class: "font-mono text-sm",
                    match &*vlad_resource.read() {
                        Some(vlad) => vlad,
                        None => "Loading...",
                    }
                }
            }

            // Display onw plog details, from plog_signal
            if let Some(plog) = plog_signal.read().as_ref() {
                PlogDisplay { plog: plog.clone() }
            } else {
                p { "Your Plog is empty." }
            }


            // Section 2: Node connections
            ConnectionsPanel { peer }

            // Section 3: Peer management
            PeerList { peer }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ConnectionStatus {
    NotConnected,
    Connecting(String),
    Connected(String),
    Error(String),
}

#[component]
fn ConnectionsPanel(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    let mut multiaddr_input = use_signal(String::new);
    let mut connection_status = use_signal(|| ConnectionStatus::NotConnected);
    let mut connecting = use_signal(|| false);
    let mut dialed_peer_id = use_signal(|| None::<String>);

    let connected_peers = use_context::<Signal<Vec<String>>>();

    use_effect(move || {
        let dialed = dialed_peer_id.read().clone();
        if let Some(dialed_id) = dialed {
            if connected_peers.read().contains(&dialed_id) {
                connection_status.set(ConnectionStatus::Connected(dialed_id));
                dialed_peer_id.set(None); // Reset for next connection
            }
        }
    });

    let peer_clone = peer;
    let handle_connect = move |_| {
        connecting.set(true);
        connection_status.set(ConnectionStatus::Connecting(
            "Attempting to connect...".to_string(),
        ));

        let addr_str = multiaddr_input();

        // Validate the multiaddr before attempting to connect
        match addr_str.parse::<Multiaddr>() {
            Ok(addr) => {
                let mut peer_id_str = None;
                for protocol in addr.iter() {
                    if let libp2p::core::multiaddr::Protocol::P2p(peer_id) = protocol {
                        peer_id_str = Some(peer_id.to_string());
                        break;
                    }
                }

                if peer_id_str.is_none() {
                    connection_status.set(ConnectionStatus::Error(
                        "Multiaddr must contain a peer ID (/p2p/...)".to_string(),
                    ));
                    connecting.set(false);
                    return;
                }
                dialed_peer_id.set(peer_id_str);

                // We need to use spawn here to avoid blocking the UI
                let peer = peer_clone;
                spawn(async move {
                    // Create a code block to scope the peer.read() so it's
                    // not held across await points
                    let network_client = {
                        let peer_guard = peer.read();
                        let Some(peer) = peer_guard.as_ref() else {
                            connection_status
                                .set(ConnectionStatus::Error("Peer not initialized".to_string()));
                            connecting.set(false);
                            return;
                        };
                        let Some(network_client) = peer.network_client.clone() else {
                            connection_status.set(ConnectionStatus::Error(
                                "Network client not initialized".to_string(),
                            ));
                            connecting.set(false);
                            return;
                        };

                        network_client
                    };

                    network_client
                        .dial(addr)
                        .await
                        .map(|_| {
                            connection_status.set(ConnectionStatus::Connecting(format!(
                                "Dialing peer at {}",
                                addr_str
                            )))
                        })
                        .unwrap_or_else(|e| {
                            connection_status
                                .set(ConnectionStatus::Error(format!("Failed to dial: {}", e)));
                        });

                    connecting.set(false);
                });
            }
            Err(e) => {
                connection_status.set(ConnectionStatus::Error(format!("Invalid Multiaddr: {}", e)));
                connecting.set(false);
            }
        }
    };

    rsx! {
        // Show controls based on status
        match connection_status() {
            ConnectionStatus::NotConnected | ConnectionStatus::Error(_) => rsx! {
                div {
                    class: "w-full pt-4",
                    h2 { class: "text-xl font-bold mb-2", "Connect to Nodes" }
                    p { "Enter a Multiaddr to connect to another node" }

                    div {
                        class: "flex space-x-2 my-2",
                        input {
                            class: "flex-grow p-2 border rounded",
                            placeholder: "/ip4/127.0.0.1/tcp/8080/p2p/...",
                            value: "{multiaddr_input}",
                            oninput: move |e| multiaddr_input.set(e.value().clone())
                        }
                        button {
                            class: "p-2 bg-blue-500 hover:bg-blue-600 text-neutral-200 rounded",
                            disabled: "{*connecting.read()}",
                            onclick: handle_connect,
                            if *connecting.read() {
                                "Connecting..."
                            } else {
                                "Dial"
                            }
                        }
                    }
                    if let ConnectionStatus::Error(ref error) = connection_status() {
                        div {
                            class: "p-2 bg-red-100 text-red-800 rounded mt-2",
                            "{error}"
                        }
                    }
                }
            },
            ConnectionStatus::Connecting(ref message) => rsx! {
                div {
                    class: "w-full pt-4",
                    h2 { class: "text-xl font-bold mb-2", "Connecting..." }
                    p { "{message}" }
                }
            },
            ConnectionStatus::Connected(ref peer_id) => rsx! {
                div {
                    class: "w-full pt-4",
                    h2 { class: "text-xl font-bold mb-2", "Connected to Peer" }
                    p { "You are connected to: {peer_id}" }
                }
            },
        }

        // Display connected peers
        if !connected_peers.read().is_empty() {
            div {
                class: "mt-4",
                h3 { class: "font-bold", "Active Connections" }
                ul {
                    class: "list-disc pl-5",
                    {connected_peers.read().iter().map(|peer_id| {
                        rsx! {
                            li {
                                key: "{peer_id}",
                                span { class: "font-mono text-sm", "{peer_id}" }
                            }
                        }
                    })}
                }
            }
        } else {
            div {
                class: "mt-4",
                p { "No active connections." }
            }
        }
    }
}

#[component]
fn PeerList(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    // Signals are always mutable
    let mut peer_vlad_input = use_signal(String::new);
    let mut searching = use_signal(|| false);
    let mut search_status = use_signal(|| None::<String>);

    let mut peer_list = use_context::<Signal<HashMap<Vlad, Option<ResolvedPlog>>>>();

    let peer_clone = peer;
    let handle_add_peer = move |_| {
        searching.set(true);
        search_status.set(Some("Searching...".to_string()));

        let vlad = peer_vlad_input();
        if vlad.trim().is_empty() {
            search_status.set(Some("VLAD cannot be empty".to_string()));
            searching.set(false);
            return;
        }

        // Try to convert string to Vlad first
        let Ok(vlad_ty) = Vlad::try_from_str(&vlad) else {
            search_status.set(Some("VLAD invalid".to_string()));
            searching.set(false);
            return;
        };

        // Insert Vlad into peer_list with None for ResolvedPlog
        // This will be updated once the peer is found
        if peer_list.read().contains_key(&vlad_ty) {
            search_status.set(Some(format!("Peer with VLAD {} already exists", vlad_ty)));
            searching.set(false);
            return;
        }
        peer_list.write().insert(vlad_ty.clone(), None);

        let vlad_bytes: Vec<u8> = vlad_ty.clone().into();

        // Add peer searching logic here
        let peer = peer_clone;
        spawn(async move {
            // Look up Vlad in DHT, if found, get Plog record and show values (plog.get_values())
            let mut network_client = {
                let peer_guard = peer.read();
                let Some(peer) = peer_guard.as_ref() else {
                    search_status.set(Some("Peer not initialized".to_string()));
                    searching.set(false);
                    return;
                };
                let Some(network_client) = peer.network_client.clone() else {
                    search_status.set(Some(
                        "Search incomplete, Network client not initialized".to_string(),
                    ));
                    searching.set(false);
                    return;
                };

                network_client
            };

            let Ok(vlad) = Vlad::try_from(vlad_bytes.as_slice()) else {
                search_status.set(Some(format!(
                    "Could not convert VLAD bytes to Vlad type: {}",
                    vlad
                )));
                searching.set(false);
                return;
            };

            if let Err(e) = network_client.subscribe(vlad.to_string()).await {
                search_status.set(Some(format!(
                    "Failed to subscribe to VLAD: {}, Error: {}",
                    vlad, e
                )));
                searching.set(false);
                return;
            }

            // try to get the DHT record for the VLAD
            let Ok(cid_bytes) = network_client.get_record(vlad_bytes).await else {
                search_status.set(Some(format!("Could not find peer with VLAD: {}", vlad)));
                searching.set(false);
                return;
            };

            let head = match multicid::Cid::try_from(cid_bytes.as_slice()) {
                Ok(head) => head,
                Err(e) => {
                    search_status.set(Some(format!(
                        "Could not get VLAD: {}, Failed to resolve plog: {}",
                        vlad, e
                    )));
                    searching.set(false);
                    return;
                }
            };

            // Rebuild the plog from the head CID by resolving the Entries
            let Ok(rebuilt_plog) = network_client.resolve_plog(&head).await else {
                search_status.set(Some(format!("Could not convert head of VLAD: {}", vlad)));
                searching.set(false);
                return;
            };

            // Add the peer details to the list
            // list.push(PeerDetails {
            //     vlad: vlad_ty,
            //     plog: Some(rebuilt_plog),
            // });
            peer_list.write().insert(vlad_ty, Some(rebuilt_plog));

            search_status.set(None);
            searching.set(false);
            peer_vlad_input.set("".to_string());
        });
    };

    let mut remove_peer = move |index: Vlad| {
        peer_list.write().remove(&index);
    };

    let peers = peer_list.read().clone();
    let has_peers = !peers.is_empty();

    let peer_items = peers.iter().enumerate().map(|(index, (vlad, maybe_plog) )| {
        let vlad_clone = vlad.clone();
        rsx! {
            li {
                key: "{index}",
                div {
                    class: "p-4 mb-4 border rounded-lg shadow-md break-all flex flex-col gap-2 bg-white", // Container styling
                    div { // Vlad Row
                        class: "flex items-center justify-between",
                        div { // Vlad Label and Value
                            class: "flex-1 flex flex-col mr-4 overflow-hidden", // Use overflow-hidden to prevent layout shifts
                            span { class: "font-bold text-lg text-gray-800", "VLAD" }
                            span { class: "font-mono text-sm text-gray-700 truncate", "{vlad.to_string()}" } // Assuming Vlad implements Display
                        }
                        button { // Copy Button
                            class: "p-1 px-2 border rounded hover:bg-gray-100 text-sm self-center flex-shrink-0",
                            // Placeholder for clipboard functionality:
                            // onclick: move |_| { /* copy to clipboard logic */ },
                            "Copy"
                        }
                    }
                    // Plog Details Section
                    if let Some(plog) = &maybe_plog {
                        div {
                            class: "mt-2 border-t pt-2 flex flex-col gap-1",
                            h4 { class: "font-semibold text-base text-gray-800", "Plog Details" }
                            // Displaying Plog entries. This part is a conceptual placeholder
                            // as the exact structure of `ResolvedPlog` and its methods (e.g., `get_values()`)
                            // are not fully defined here.
                            //
                            // Assuming `ResolvedPlog` has an `entries` field which is a `Vec<Entry>`,
                            // and `Entry` has `key: String` and `value: String`.
                            div {
                                class: "text-sm text-gray-600",
                                if plog.log.entries.is_empty() {
                                    "No entries in Plog."
                                } else {
                                        span { "Plog contains {plog.log.entries.len()} entries." }
                                        // Optionally display first few entries for brevity:
                                        // This would look something like:
                                        // {
                                        //     plog.entries.iter().take(3).map(|entry| {
                                        //         rsx! {
                                        //             div {
                                        //                 class: "ml-4 flex gap-1",
                                        //                 span { class: "font-mono text-xs", "{entry.key}:" }
                                        //                 span { class: "truncate", "{entry.value}" }
                                        //             }
                                        //         }
                                        //     })
                                        // }
                                        PlogDisplay { plog: plog.log.clone() }
                                    }
                            }
                        }
                    } else {
                        div {
                            class: "mt-2 text-sm text-gray-500",
                            "No Plog available for this peer."
                        }
                    }
                    // Remove Peer Button
                    div {
                        class: "mt-3 text-right",
                        button {
                            class: "p-1 px-2 border border-red-500 rounded text-red-500 hover:bg-red-100 text-sm",
                            onclick: move |_| remove_peer(vlad_clone.clone()), // clone again b/c it's  is captured from outer scope in this closure
                            "Remove"
                        }
                    }
                }
            }
        }
    });

    // State-dependent button text
    let button_text = if *searching.read() {
        "Searching..."
    } else {
        "Add Peer"
    };

    // Peer list section
    let peer_list_section = if has_peers {
        rsx! {
            div {
                class: "mt-4",
                h3 { class: "font-bold", "Connected Peers" }
                ul {
                    class: "list-disc pl-5",
                    {peer_items}
                }
            }
        }
    } else {
        rsx! {}
    };

    // Always keep as much logic outside of the rsx! macro as possible.
    rsx! {
        div {
            class: "w-full pt-4",
            h2 { class: "text-xl font-bold mb-2", "Peers" }
            p { "Add peers by their VLAD to interact with them" }

            div {
                class: "flex space-x-2 my-2",
                input {
                    class: "flex-grow p-2 border rounded",
                    placeholder: "Enter peer VLAD...",
                    value: "{peer_vlad_input}",
                    oninput: move |e| peer_vlad_input.set(e.value().clone())
                }
                button {
                    class: "p-2 bg-blue-500 hover:bg-blue-600 text-white rounded",
                    disabled: "{*searching.read()}",
                    onclick: handle_add_peer,
                    "{button_text}"
                }
            }

            if let Some(status) = search_status() {
                div {
                    class: "p-2 bg-gray-100 rounded mt-2",
                    "Search Status: {status}"
                }
            }

            {peer_list_section}
        }
    }
}

// A component to display the plog
#[component]
fn PlogDisplay(plog: provenance_log::Log) -> Element {
    rsx! {
        div {
            class: "p-4 mb-4 border rounded-lg shadow-md bg-neutral-100 text-green-800",
            h3 { class: "font-bold text-lg mb-2", "Plog Entries" }
            if plog.entries.is_empty() {
                p { "No entries in Plog." }
            } else {
                ul {
                    class: "list-disc pl-5",
                    for (idx, maybe_verified) in plog.verify().enumerate() {
                        match maybe_verified {
                            Ok((_count, entry, _kvp)) => {
                                // Display the entry count
                                rsx! {
                                    li {
                                        class: "mb-2",
                                        span { class: "font-mono text-xs", "Entry {idx}: " }
                                        // Display the key-value pairs in the entry
                                        DisplayEntry { entry: entry.clone() }
                                    }
                                }
                            }
                            Err(e) => {
                                // Handle error case
                                rsx! {
                                    li {
                                        class: "mb-2 text-red-500",
                                        "Error verifying entry: {e}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DisplayEntry(entry: provenance_log::Entry) -> Element {
    rsx! {
        div {
            class: "mb-2 text-left",
            div {
                class: "flex flex-col items-baseline justify-between",
                for op in entry.ops() {
                    DisplayOp { op: op.clone() }
                }
            }
        }
    }
}

#[component]
fn DisplayOp(op: provenance_log::Op) -> Element {
    rsx! {
        div {
            class: "flex gap-2",
            {
                match op {
                    provenance_log::Op::Noop(key) => {
                        rsx! {
                            span { class: "font-mono text-xs", "No operation for key: {key}" }
                        }
                    }
                    provenance_log::Op::Delete(key) => {
                        rsx! {
                            span { class: "font-mono text-xs text-red-500", "Deleted key: {key}" }
                        }
                    }
                    provenance_log::Op::Update(key, value) => {
                        match value {
                            provenance_log::Value::Nil => {
                                rsx! {
                                    span { class: "font-mono text-xs", "{key} Nil" }
                                }
                            }
                            provenance_log::Value::Str(s) => {
                                rsx! {
                                    span { class: "font-mono text-xs", "{key} {s}" }
                                }
                            }
                            provenance_log::Value::Data(data) => {
                                rsx! {
                                    span { class: "font-mono text-xs", "{key} {data.len()} bytes" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
