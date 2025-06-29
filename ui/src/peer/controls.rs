//! This Dioxus component allows users to interact with the Plog controls of a peer.
//!
//! - It shows whether or not a peer has a plog
//! - It provides a button to create a plog if it doesn't exist.
//! - If it does exist, it displays the plog's vlad as a string for user informatio, with a
//!   clipboard button to copy the vlad to the clipboard.
//!
//! Platform agnostic, so doesnt contain tokio or other platform specific code.
use bs_peer::peer::{Client, DefaultBsPeer, ResolvedPlog, ResolverExt};
use bs_peer::utils::create_default_scripts;
use dioxus::prelude::*;
use libp2p::Multiaddr;
use multicid::Vlad;

use crate::wallet::KeyMan;

#[component]
pub fn PlogControls(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    // should show peer.plog.vlad as string for FYI.
    let has_plog = move || {
        peer.read()
            .as_ref()
            .map(|p| p.plog().is_some())
            .unwrap_or(false)
    };

    let vlad_string = move || {
        peer.read()
            .as_ref()
            .and_then(|p| p.plog().as_ref().map(|plog| plog.vlad.to_string()))
            .unwrap_or_else(|| "No Plog available".to_string())
    };

    rsx! {
        div {
            class: "w-full h-full flex flex-col items-center justify-center",
            h1 { "Plog Details" }
            p { "This is you Verifiable Long-Lived Address (VLAD). It will stay the same, even when you rotate keys. No blockchain required, it's all peer to peer." }
            div {
                class: "m-2 p-2 bg-green-50/5 border-2 border-green-500/50 rounded-lg w-full break-all",
                span {
                    class: "font-mono text-sm",
                    "{vlad_string()}"
                }
            }

            // Section 2: Node connections
            ConnectionsPanel { peer: peer.clone() }

            // Section 3: Peer management
            PeerList { peer: peer.clone() }
        }
    }
}

#[component]
fn ConnectionsPanel(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    let mut multiaddr_input = use_signal(String::new);
    let mut connection_status = use_signal(|| None::<String>);
    let mut connecting = use_signal(|| false);

    let connected_peers = use_context::<Signal<Vec<String>>>();

    let handle_connect = move |_| {
        connecting.set(true);
        connection_status.set(None);

        let addr_str = multiaddr_input();

        // Validate the multiaddr before attempting to connect
        match addr_str.parse::<Multiaddr>() {
            Ok(addr) => {
                // We need to use spawn here to avoid blocking the UI
                spawn(async move {
                    // Create a code block to scope the peer.read() so it's
                    // not held acros await points
                    let network_client = {
                        let peer_guard = peer.read();

                        let Some(peer_ref) = peer_guard.as_ref() else {
                            connection_status.set(Some("Peer not initialized".to_string()));
                            connecting.set(false);
                            return;
                        };

                        let Some(network_client) = peer_ref.network_client.clone() else {
                            connection_status
                                .set(Some("Network client not initialized".to_string()));
                            connecting.set(false);
                            return;
                        };

                        network_client
                    };

                    let result = match network_client.dial(addr).await {
                        Ok(_) => "Dialing peer...".to_string(),
                        Err(e) => format!("Failed to dial: {}", e),
                    };

                    connection_status.set(Some(result));
                    connecting.set(false);
                });
            }
            Err(e) => {
                connection_status.set(Some(format!("Invalid Multiaddr: {}", e)));
                connecting.set(false);
            }
        }
    };

    rsx! {
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
                    class: "p-2 bg-blue-500 hover:bg-blue-600 text-white rounded",
                    disabled: "{connecting}",
                    onclick: handle_connect,
                    if *connecting.read() {
                        "Connecting..."
                    } else {
                        "Connect"
                    }
                }
            }

            if let Some(status) = connection_status() {
                div {
                    class: "p-2 bg-gray-100 rounded mt-2",
                    "{status}"
                }
            }

            div {
                class: "mt-4",
                h3 { class: "font-bold", "Active Connections" }
                ul {
                    class: "list-disc pl-5",
                    {connected_peers().iter().map(|peer_id| {
                        rsx! {
                            li {
                                key: "{peer_id}",
                                span { class: "font-mono text-sm", "{peer_id}" }
                            }
                        }
                    })}
                }
            }
        }
    }
}

#[component]
fn PeerList(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    // simple sturct to hold Vlad and Plog details
    #[derive(Clone, Debug)]
    struct PeerDetails {
        vlad: Vlad,
        plog: Option<ResolvedPlog>,
    }

    // Signals are always mutable
    let mut peer_vlad_input = use_signal(String::new);
    let mut peer_list = use_signal(Vec::<PeerDetails>::new);
    let mut searching = use_signal(|| false);
    let mut search_status = use_signal(String::new);

    let handle_add_peer = move |_| {
        searching.set(true);
        search_status.set("Searching...".to_string());

        let vlad = peer_vlad_input();
        if vlad.trim().is_empty() {
            search_status.set("VLAD cannot be empty".to_string());
            searching.set(false);
            return;
        }

        // Try to convert string to Vlad first
        let Ok(vlad_ty) = Vlad::try_from_str(&vlad) else {
            search_status.set("VLAD invalid".to_string());
            searching.set(false);
            return;
        };

        let vlad_bytes: Vec<u8> = vlad_ty.clone().into();

        // Add peer searching logic here
        spawn(async move {
            // Look up Vlad in DHT, if found, get Plog record and show values (plog.get_values())
            let network_client = {
                let peer_guard = peer.read();

                let Some(peer_ref) = peer_guard.as_ref() else {
                    search_status.set("Search incomplete, peer not initialized".to_string());
                    searching.set(false);
                    return;
                };

                let Some(network_client) = peer_ref.network_client.clone() else {
                    search_status
                        .set("Search incomplete, Network client not initialized".to_string());
                    searching.set(false);
                    return;
                };

                network_client
            };

            let Ok(cid_bytes) = network_client.get_record(vlad_bytes).await else {
                search_status.set(format!("Could not find peer with VLAD: {}", vlad));
                searching.set(false);
                return;
            };

            let head = match multicid::Cid::try_from(cid_bytes.as_slice()) {
                Ok(head) => head,
                Err(e) => {
                    search_status.set(format!(
                        "Could not get VLAD: {}, Failed to resolve plog: {}",
                        vlad, e
                    ));
                    searching.set(false);
                    return;
                }
            };

            // Rebuild the plog from the head CID by resolving the Entries
            let Ok(rebuilt_plog) = network_client.resolve_plog(&head).await else {
                search_status.set(format!("Could not convert head of VLAD: {}", vlad));
                searching.set(false);
                return;
            };

            // Add the peer details to the list
            let mut list = peer_list.write();
            list.push(PeerDetails {
                vlad: vlad_ty,
                plog: Some(rebuilt_plog),
            });

            search_status.set(format!("Found and added peer: {}", vlad));
            searching.set(false);
            peer_vlad_input.set("".to_string());
        });
    };

    let mut remove_peer = move |index: usize| {
        let mut list = peer_list.write();
        if index < list.len() {
            list.remove(index);
        }
    };

    // Get current state values outside of RSX
    let is_searching = *searching.read();
    let peers = peer_list.read().clone();
    let has_peers = !peers.is_empty();

    // Build peer list items outside of RSX
    let peer_items = peers.iter().enumerate().map(|(index, plog_details)| {
        let index_clone = index;
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
                            span { class: "font-mono text-sm text-gray-700 truncate", "{plog_details.vlad.to_string()}" } // Assuming Vlad implements Display
                        }
                        button { // Copy Button
                            class: "p-1 px-2 border rounded hover:bg-gray-100 text-sm self-center flex-shrink-0",
                            // Placeholder for clipboard functionality:
                            // onclick: move |_| { /* copy to clipboard logic */ },
                            "Copy"
                        }
                    }
                    // Plog Details Section
                    if let Some(plog) = &plog_details.plog {
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
                            onclick: move |_| remove_peer(index_clone), // index_clone is captured from outer scope
                            "Remove"
                        }
                    }
                }
            }
        }
    });

    // State-dependent button text
    let button_text = if is_searching {
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
                    disabled: "{is_searching}",
                    onclick: handle_add_peer,
                    "{button_text}"
                }
            }

            div {
                class: "p-2 bg-gray-100 rounded mt-2",
                "Search Status: {search_status()}"
            }

            {peer_list_section}
        }
    }
}

// Helper function
// Create a code block to scope the peer.read() so it's
// not held acros await points
fn network_client_clone(
    peer: Signal<Option<DefaultBsPeer<KeyMan>>>,
    mut connection_status: Signal<Option<String>>,
    mut connecting: Signal<bool>,
) -> Option<Client> {
    let peer_guard = peer.read();

    let Some(peer_ref) = peer_guard.as_ref() else {
        connection_status.set(Some("Peer not initialized".to_string()));
        connecting.set(false);
        return None;
    };

    let Some(network_client) = peer_ref.network_client.clone() else {
        connection_status.set(Some("Network client not initialized".to_string()));
        connecting.set(false);
        return None;
    };

    Some(network_client)
}
