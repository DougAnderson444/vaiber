//! This Dioxus component allows users to interact with the Plog controls of a peer.
//!
//! - It shows whether or not a peer has a plog
//! - It provides a button to create a plog if it doesn't exist.
//! - If it does exist, it displays the plog's vlad as a string for user informatio, with a
//!   clipboard button to copy the vlad to the clipboard.
//!
//! Platform agnostic, so doesnt contain tokio or other platform specific code.
use bs_peer::peer::DefaultBsPeer;
use bs_peer::utils::create_default_scripts;
use dioxus::prelude::*;
use libp2p::Multiaddr;

use crate::wallet::KeyMan;

#[component]
pub fn PlogControls(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    let creating_plog = use_signal(|| false);

    let (lock_script, unlock_script) = create_default_scripts();
    // if peer.plog is_none, we can create a plog using generate:
    // peer.generate(&fixture.lock_script, &fixture.unlock_script).await;

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
            class: "p-8 w-full h-full flex flex-col items-center justify-center",
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

    let handle_connect = move |_| {
        connecting.set(true);
        connection_status.set(None);

        let addr_str = multiaddr_input();

        // Validate the multiaddr before attempting to connect
        match addr_str.parse::<Multiaddr>() {
            Ok(addr) => {
                // We need to use spawn here to avoid blocking the UI
                spawn(async move {
                    let result = match peer.read().as_ref() {
                        Some(p) => {
                            match p.network_client.as_ref() {
                                Some(client) => {
                                    // We need to clone to get around borrowing rules
                                    let client_mut = client.clone();
                                    match client_mut.dial(addr).await {
                                        Ok(_) => "Connected successfully!",
                                        Err(e) => &format!("Failed to connect: {}", e),
                                    }
                                }
                                None => "Network client not initialized",
                            }
                        }
                        None => "Peer not initialized",
                    };

                    connection_status.set(Some(result.to_string()));
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
        }
    }
}

#[component]
fn PeerList(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    // Signals are always mutable
    let mut peer_vlad_input = use_signal(String::new);
    let mut peer_list = use_signal(Vec::<String>::new);
    let mut searching = use_signal(|| false);
    let mut search_status = use_signal(|| None::<String>);

    let handle_add_peer = move |_| {
        searching.set(true);
        search_status.set(None);

        let vlad = peer_vlad_input();
        if vlad.trim().is_empty() {
            search_status.set(Some("VLAD cannot be empty".to_string()));
            searching.set(false);
            return;
        }

        // Add peer searching logic here
        spawn(async move {
            // This would be a call to find a peer by VLAD
            // For now, just simulating the functionality

            if vlad.starts_with("vlad") || vlad.starts_with("v1:") {
                peer_list.write().push(vlad.clone());
                search_status.set(Some(format!("Found and added peer: {}", vlad)));
            } else {
                search_status.set(Some(format!("Could not find peer with VLAD: {}", vlad)));
            }

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
    let current_status = search_status();
    let peers = peer_list.read().clone();
    let has_peers = !peers.is_empty();

    // Build peer list items outside of RSX
    let peer_items = peers.iter().enumerate().map(|(index, vlad)| {
        let index_clone = index;
        rsx! {
            li {
                key: "{index}",
                div {
                    class: "flex justify-between items-center",
                    span { class: "font-mono text-sm", "{vlad}" }
                    button {
                        class: "text-red-500",
                        onclick: move |_| remove_peer(index_clone),
                        "Remove"
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

    // State-dependent status message
    let status_message = current_status.map(|status| {
        rsx! {
            div {
                class: "p-2 bg-gray-100 rounded mt-2",
                "{status}"
            }
        }
    });

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

            {status_message}
            {peer_list_section}
        }
    }
}
