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
use provenance_log::{Key as ProvenanceKey, Log, Script};
use std::collections::HashMap;
use std::path::PathBuf;

const VLAD_STORAGE_KEY: &str = "VLAD_STORAGE_KEY";

#[component]
pub fn Peer(platform_content: Element, base_path: Option<PathBuf>) -> Element {
    let storage = use_context::<StorageProvider>();
    let (lock_script, unlock_script) = create_default_scripts();

    let key_manager = use_context::<Signal<Option<KeyMan>>>();
    let mut bs_peer_signal = use_signal(|| None::<DefaultBsPeer<KeyMan>>);
    let mut plog_signal = use_signal(|| None::<Log>);
    let mut peer_address = use_signal(|| None::<String>);
    let mut connected_peers = use_signal(Vec::<String>::new);
    let mut ack_list = use_signal(Vec::<Vlad>::new);
    let mut peer_list = use_signal(HashMap::<Vlad, Option<ResolvedPlog>>::new);

    use_context_provider(move || peer_list);
    use_context_provider(|| connected_peers);
    use_context_provider(|| plog_signal);

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
    let lock_script_clone = lock_script.clone();
    let storage_clone = storage.clone();
    let base_path_clone = base_path.clone();
    let bs_peer_resource = use_resource(move || {
        let km = km.clone();
        let storage = storage_clone.clone();
        let lock_clone = lock_script_clone.clone();
        let unlock_clone = unlock_script_clone.clone();
        let bath_path_clone = base_path_clone.clone();
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

            let plog_loaded = if storage.exists(VLAD_STORAGE_KEY) && !cfg!(feature = "dev") {
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

            if !plog_loaded {
                peer.generate(&lock_clone, &unlock_clone.clone())
                    .await
                    .unwrap_or_else(|e| tracing::error!("Failed to generate Plog: {}", e));
                if let Some(plog_data) = peer.plog() {
                    let plog_bytes: Vec<u8> = plog_data.into();
                    storage
                        .save(VLAD_STORAGE_KEY, &plog_bytes)
                        .unwrap_or_else(|e| {
                            tracing::error!("Failed to save Plog to storage: {}", e);
                        });
                };
            }

            if let Some(plog) = peer.plog() {
                plog_signal.set(Some(plog.clone()));
            } else {
                tracing::error!("Plog is not initialized.");
            }

            let peer_clone = peer.clone();
            let update_dht = move || {
                let mut peer_clone_inner = peer_clone.clone();
                async move {
                    if let Err(e) = peer_clone_inner.record_plog_to_dht().await {
                        tracing::error!("Failed to publish Plog records: {}", e);
                    } else {
                        tracing::info!("Plog records published to DHT successfully.");
                    }
                    if let Err(e) = peer_clone_inner.record_peer_id_to_dht().await {
                        tracing::error!("Failed to publish PeerId record: {}", e);
                    } else {
                        tracing::info!("PeerId record published to DHT successfully.");
                    }
                }
            };

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
                            connected_peers.write().push(peer.to_string());
                            update_dht().await;
                        }
                        PublicEvent::ConnectionClosed { peer, cause } => {
                            tracing::info!(
                                "Connection closed with peer: {}, cause: {:?}",
                                peer,
                                cause
                            );
                            connected_peers.write().retain(|p| p != &peer.to_string());
                        }
                        PublicEvent::Message { topic, data, .. } => {
                            tracing::info!("Received message topic: {}, data: {:?}", topic, data);
                            if let Ok(vlad) = Vlad::try_from_str(&topic) {
                                if ack_list.read().contains(&vlad) {
                                    return;
                                }
                                ack_list.write().push(vlad.clone());

                                if let Some(network_client) = peer_clone.network_client.as_ref() {
                                    let Ok(head) = multicid::Cid::try_from(data.as_slice()) else {
                                        tracing::warn!("Invalid VLAD: {}", vlad);
                                        return;
                                    };
                                    // Retry logic with exponential backoff
                                    let mut retries = 0;
                                    loop {
                                        match network_client.resolve_plog(&head).await {
                                            Ok(plog) => {
                                                peer_list.with_mut(|map| {
                                                    map.insert(vlad.clone(), Some(plog))
                                                });
                                                break;
                                            }
                                            Err(e) => {
                                                if retries >= 3 {
                                                    tracing::error!("Failed to resolve plog after 3 retries: {}", e);
                                                    break;
                                                }
                                                retries += 1;
                                                tokio::time::sleep(std::time::Duration::from_secs(
                                                    2u64.pow(retries),
                                                ))
                                                .await;
                                            }
                                        }
                                    }
                                    // Send ACK back to the sender
                                    let ack_topic = format!("ack/{}", topic);
                                    if let Err(e) =
                                        network_client.publish(data.clone(), ack_topic).await
                                    {
                                        tracing::error!("Failed to publish ACK: {}", e);
                                    }
                                }
                            }
                        }
                        PublicEvent::Swarm(Libp2pEvent::PutRecordRequest { source }) => {
                            tracing::info!("Received PutRecordRequest from: {}", source);
                            if let Some(network_client) = peer_clone.network_client.as_ref() {
                                let peer_list_clone = peer_list.read().clone();
                                for (vlad, plog) in peer_list_clone.iter() {
                                    if plog.is_some() {
                                        continue;
                                    }
                                    #[cfg(not(target_arch = "wasm32"))]
                                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                                    let vlad_bytes: Vec<u8> = vlad.clone().into();
                                    let cid_bytes = {
                                        let mut retries = 0;
                                        loop {
                                            match network_client
                                                .get_record(vlad_bytes.clone())
                                                .await
                                            {
                                                Ok(bytes) => break Ok(bytes),
                                                Err(e) => {
                                                    if retries >= 3 {
                                                        break Err(e);
                                                    }
                                                    retries += 1;
                                                    tokio::time::sleep(
                                                        std::time::Duration::from_secs(
                                                            2u64.pow(retries),
                                                        ),
                                                    )
                                                    .await;
                                                }
                                            }
                                        }
                                    };
                                    let Ok(cid_bytes) = cid_bytes else {
                                        continue;
                                    };
                                    let Ok(head) = multicid::Cid::try_from(cid_bytes.as_slice())
                                    else {
                                        continue;
                                    };
                                    let Ok(_head_bytes) = network_client.resolve(&head).await
                                    else {
                                        continue;
                                    };
                                    let vlad_clone = vlad.clone();
                                    let network_client_clone = network_client.clone();
                                    spawn(async move {
                                        if let Ok(resolved_plog) =
                                            network_client_clone.resolve_plog(&head).await
                                        {
                                            peer_list.with_mut(|map| {
                                                map.insert(vlad_clone.clone(), Some(resolved_plog))
                                            });
                                        }
                                    });
                                }
                            }
                        }
                        PublicEvent::Ack { topic, .. } => {
                            tracing::info!("Received ACK for topic: {}", topic);
                            if let Ok(vlad) = Vlad::try_from_str(&topic) {
                                ack_list.write().retain(|v| v != &vlad);
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

    rsx! {
        div {
            class: "p-8 w-full h-full flex flex-col items-center justify-center gap-6 bg-neutral-50",
            h1 { class: "text-3xl font-bold mb-2 text-green-800", "PeerPiper vaiber" }
            if let Some(_) = &*bs_peer_resource.read() {
                div {
                    class: "w-full max-w-4xl flex flex-col md:flex-row gap-8 justify-stretch",
                    // My Plog column
                    div {
                        class: "flex flex-col gap-6 flex-1",
                        MyPlogSection {
                            bs_peer_signal: bs_peer_signal,
                            plog_signal: plog_signal,
                            unlock_script: unlock_script.clone(),
                            peer_address: peer_address()
                        }
                    }
                    // Platform content column
                    div {
                        class: "flex-1 flex flex-col gap-4",
                        {platform_content}
                    }
                }
                // Connections section (full width below columns)
                div {
                    class: "w-full max-w-4xl flex flex-col gap-6 mt-8",
                    ConnectionsSection { peer: bs_peer_signal }
                }
                // Tracked Peers section (full width below connections)
                div {
                    class: "w-full max_w_4xl flex flex-col gap-6 mt-4",
                    PeerListSection { peer: bs_peer_signal }
                }
            } else {
                div { class: "py-8 text-xl text-gray-400", "Initializing peer..." }
            }
        }
    }
}

// === SECTION: MyPlogSection ===

#[component]
fn MyPlogSection(
    bs_peer_signal: Signal<Option<DefaultBsPeer<KeyMan>>>,
    plog_signal: Signal<Option<Log>>,
    unlock_script: String,
    peer_address: Option<String>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-6 bg-white border border-green-100 rounded-lg p-6 shadow-sm",
            h2 { class: "text-2xl font-bold text-green-800 mb-2", "My Plog Details" }
            PlogControls { peer: bs_peer_signal }
            AddOperationForm {
                bs_peer_signal: bs_peer_signal,
                unlock_script: unlock_script,
            }
            if let Some(addr) = peer_address {
                div {
                    class: "mt-2 text-xs text-center",
                    span { class: "font-semibold text-gray-700", "Your Node Address:" }
                    p {
                        class: "m-2 p-2 bg-green-50 border border-green-400 rounded-lg w-full break-all text-green-900 font-mono",
                        "{addr}"
                    }
                }
            }
        }
    }
}

// === SECTION: ConnectionsSection ===

#[component]
fn ConnectionsSection(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-6 bg-white border border-blue-100 rounded-lg p-6 shadow-sm",
            h2 { class: "text-2xl font-bold text-blue-800 mb-2", "Connections" }
            ConnectionsPanel { peer }
        }
    }
}

// === SECTION: PeerListSection ===

#[component]
fn PeerListSection(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-6 bg-white border border-gray-100 rounded-lg p-6 shadow-sm",
            h2 { class: "text-2xl font-bold text-gray-800 mb-2", "Tracked Peers" }
            PeerList { peer }
        }
    }
}

// === SECTION: PlogControls (VLAD and Plog entries only!) ===

#[component]
pub fn PlogControls(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
    let plog_signal = use_context::<Signal<Option<provenance_log::Log>>>();

    let vlad_resource = use_resource({
        move || async move {
            let binding = peer.read().as_ref().unwrap().plog();
            let Some(ref plog) = binding else {
                return "No Plog available".to_string();
            };
            plog.vlad.to_string()
        }
    });

    rsx! {
        div {
            class: "flex flex-col gap-4 bg-white border border-green-100 rounded-lg p-4 shadow-sm",
            h3 { class: "text-xl font-bold text-green-700 mb-1", "Plog Entries" }
            p { class: "text-xs text-gray-600", "Your Verifiable Long-Lived Address (VLAD):" }
            div {
                class: "bg-green-50 border border-green-300 rounded p-2 font-mono text-sm text-green-800 break-all select-all",
                match &*vlad_resource.read() {
                    Some(vlad) => vlad,
                    None => "Loading...",
                }
            }
            if let Some(plog) = plog_signal.read().as_ref() {
                PlogDisplay { plog: plog.clone() }
            } else {
                p { class: "italic text-gray-400", "Your Plog is empty." }
            }
        }
    }
}

// === SECTION: Add Operation ===

#[component]
fn AddOperationForm(
    bs_peer_signal: Signal<Option<DefaultBsPeer<KeyMan>>>,
    unlock_script: String,
) -> Element {
    let storage = use_context::<StorageProvider>();
    let mut plog_signal = use_context::<Signal<Option<Log>>>();

    let mut key = use_signal(String::new);
    let mut value = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);

    let handle_submit = move |_| {
        // e.prevent_default();
        submitting.set(true);
        let k = key().trim().to_string();
        let v = value().trim().to_string();
        if k.is_empty() || v.is_empty() {
            error.set(Some("Key and Value cannot be empty.".into()));
            submitting.set(false);
            return;
        }
        error.set(None);

        let mut additional_ops = vec![];
        additional_ops.push(OpParams::UseStr {
            key: ProvenanceKey::try_from(k.clone()).unwrap_or_default(),
            s: v.clone(),
        });

        if let Some(peer) = bs_peer_signal.read().as_ref() {
            let mut peer_clone = peer.clone();
            let storage = storage.clone();
            let unlock_script = unlock_script.clone();
            let mut key = key;
            let mut value = value;
            let mut submitting = submitting;
            spawn(async move {
                let update_cfg = bs::update::Config::builder()
                    .unlock(Script::Code(provenance_log::Key::default(), unlock_script))
                    .entry_signing_key(PubkeyParams::KEY_PATH.into())
                    .additional_ops(additional_ops)
                    .build();

                if let Err(e) = peer_clone.update(update_cfg).await {
                    tracing::error!("Failed to update plog: {}", e); // TODO: Need to show this to the user.
                                                                     // But more importantly, why would verification fail for local plog?
                } else {
                    bs_peer_signal.set(Some(peer_clone.clone()));
                }
                if let Some(ref plog) = peer_clone.plog() {
                    let plog_bytes: Vec<u8> = plog.clone().into();
                    if let Err(e) = storage.save(VLAD_STORAGE_KEY, &plog_bytes) {
                        tracing::error!("Failed to save Plog to storage: {}", e);
                    } else {
                        plog_signal.set(Some(plog.clone()));
                    }
                }
                submitting.set(false);
                key.set(String::new());
                value.set(String::new());
            });
        } else {
            error.set(Some("Peer is not initialized.".to_string()));
            submitting.set(false);
        }
    };

    rsx! {
        form {
            class: "flex flex-col gap-2 bg-white border border-green-200 rounded-lg p-4 shadow-sm",
            onsubmit: handle_submit,
            h4 { class: "text-lg font-semibold text-green-700", "Add Operation" }
            div {
                class: "flex gap-2 w-full",
                input {
                    class: "flex-1 p-2 border rounded focus:outline-none focus:ring-2 focus:ring-green-400 font-mono text-xs",
                    placeholder: "Key",
                    name: "key",
                    value: "{key}",
                    autocomplete: "off",
                    oninput: move |e| {
                        let mut input = e.value().clone();
                        if !input.starts_with('/') {
                            input.insert(0, '/');
                        }
                        input = input.replace(" ", "/");
                        input = input.replace(|c: char| !c.is_alphanumeric() && c != '/', "/");
                        key.set(input.clone());
                    },
                }
                input {
                    class: "flex-1 p-2 border rounded focus:outline-none focus:ring-2 focus:ring-green-400 font-mono text-xs",
                    placeholder: "Value",
                    name: "value",
                    value: "{value}",
                    autocomplete: "off",
                    oninput: move |e| value.set(e.value().clone()),
                }
                button {
                    class: "px-3 py-2 bg-green-500 hover:bg-green-600 text-white rounded font-bold transition",
                    r#type: "submit",
                    disabled: *submitting.read(),
                    if *submitting.read() { "Adding..." } else { "Add" }
                }
            }
            if let Some(err) = error() {
                p { class: "text-red-500 text-xs mt-1", "{err}" }
            }
        }
    }
}

// === SECTION: ConnectionsPanel ===

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
                dialed_peer_id.set(None);
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

                let peer = peer_clone;
                spawn(async move {
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
        div {
            class: "w-full pt-2",
            match connection_status() {
                ConnectionStatus::NotConnected | ConnectionStatus::Error(_) => rsx! {
                    div {
                        h3 { class: "font-bold mb-2", "Connect to Nodes" }
                        p { class: "text-xs text-gray-600", "Enter a Multiaddr to connect to another node" }
                        div {
                            class: "flex gap-2 mb-3",
                            input {
                                class: "flex-grow p-2 border rounded text-xs font-mono",
                                placeholder: "/ip4/127.0.0.1/tcp/8080/p2p/...",
                                value: "{multiaddr_input}",
                                oninput: move |e| multiaddr_input.set(e.value().clone())
                            }
                            button {
                                class: "p-2 bg-blue-500 hover:bg-blue-600 text-white rounded font-bold",
                                disabled: *connecting.read(),
                                onclick: handle_connect,
                                if *connecting.read() { "Connecting..." } else { "Dial" }
                            }
                        }
                        if let ConnectionStatus::Error(ref error) = connection_status() {
                            div { class: "p-2 bg-red-100 text-red-800 rounded mb-2 text-xs", "{error}" }
                        }
                    }
                },
                ConnectionStatus::Connecting(ref message) => rsx! {
                    div {
                        h3 { class: "font-bold mb-2", "Connecting..." }
                        p { class: "text-xs", "{message}" }
                    }
                },
                ConnectionStatus::Connected(ref peer_id) => rsx! {
                    div {
                        h3 { class: "font-bold mb-2", "Connected to Node(s)" }
                        p { class: "text-xs", "You are connected to: {peer_id}" }
                    }
                }
            }
            if !connected_peers.read().is_empty() {
                div {
                    class: "mt-3",
                    h4 { class: "font-semibold text-green-700", "Active Connections" }
                    ul {
                        class: "list-disc pl-5 text-xs",
                        for peer_id in connected_peers.read().iter() {
                            li {
                                key: "{peer_id}",
                                span { class: "font-mono", "{peer_id}" }
                            }
                        }
                    }
                }
            } else {
                div { class: "mt-3 text-xs text-gray-400", "No active connections." }
            }
        }
    }
}

// === SECTION: PeerList ===

#[component]
fn PeerList(peer: Signal<Option<DefaultBsPeer<KeyMan>>>) -> Element {
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
            search_status.set(Some("VLAD cannot be empty".into()));
            searching.set(false);
            return;
        }
        let Ok(vlad_ty) = Vlad::try_from_str(&vlad) else {
            search_status.set(Some("VLAD invalid".into()));
            searching.set(false);
            return;
        };
        if peer_list.read().contains_key(&vlad_ty) {
            search_status.set(Some(format!("Peer with VLAD {} already exists", vlad_ty)));
            searching.set(false);
            return;
        }
        peer_list.with_mut(|map| map.insert(vlad_ty.clone(), None));
        let vlad_bytes: Vec<u8> = vlad_ty.clone().into();

        let peer = peer_clone;
        spawn(async move {
            let network_client = {
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
            let cid_bytes = {
                let mut retries = 0;
                loop {
                    match network_client.get_record(vlad_bytes.clone()).await {
                        Ok(bytes) => break Ok(bytes),
                        Err(e) => {
                            if retries >= 5 {
                                // Increased retries for DHT propagation
                                tracing::error!("Failed to get record after 5 retries: {}", e);
                                break Err(e);
                            }
                            retries += 1;
                            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(retries)))
                                .await;
                        }
                    }
                }
            };
            let Ok(cid_bytes) = cid_bytes else {
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
            let Ok(rebuilt_plog) = network_client.resolve_plog(&head).await else {
                search_status.set(Some(format!("Could not convert head of VLAD: {}", vlad)));
                searching.set(false);
                return;
            };
            peer_list.with_mut(|map| map.insert(vlad_ty, Some(rebuilt_plog)));
            search_status.set(None);
            searching.set(false);
            peer_vlad_input.set("".to_string());
        });
    };

    let remove_peer = move |index: Vlad| {
        peer_list.with_mut(|map| map.remove(&index));
    };

    let peers = peer_list.read().clone();
    let has_peers = !peers.is_empty();

    rsx! {
        div {
            class: "w-full pt-4",
            h3 { class: "text-xl font-bold mb-2 text-green-700", "Peers" }
            p { class: "text-xs text-gray-600", "Add peers by their VLAD to interact with them" }
            div {
                class: "flex gap-2 mb-2",
                input {
                    class: "flex-grow p-2 border rounded font-mono text-xs",
                    placeholder: "Enter peer VLAD...",
                    value: "{peer_vlad_input}",
                    oninput: move |e| peer_vlad_input.set(e.value().clone())
                }
                button {
                    class: "p-2 bg-blue-500 hover:bg-blue-600 text-white rounded",
                    disabled: *searching.read(),
                    onclick: handle_add_peer,
                    if *searching.read() { "Searching..." } else { "Add Peer" }
                }
            }
            if let Some(status) = search_status() {
                div {
                    class: "p-2 bg-red-100 text-red-800 rounded mb-2 text-xs",
                    "Search Status: {status}"
                }
            }
            if has_peers {
                div {
                    class: "mt-2",
                    h4 { class: "font-semibold", "Following these Plogs" }
                    PeerItems { peers: peers.clone() }
                }
            }
        }
    }
}

#[component]
fn PeerItems(peers: HashMap<Vlad, Option<ResolvedPlog>>) -> Element {
    rsx! {
        ul {
            class: "list-none flex flex-col gap-2",
            for (index, (vlad, maybe_plog)) in peers.iter().enumerate() {
                li {
                    key: "{index}",
                    div {
                        class: "p-3 border rounded-lg shadow-sm break-all bg-neutral-50 flex flex-col gap-2",
                        div {
                            class: "flex items-center justify-between gap-2",
                            div {
                                class: "flex-1 flex flex-col gap-0.5",
                                span { class: "font-semibold text-green-900 text-xs", "VLAD" }
                                span { class: "font-mono text-xs text-gray-700 truncate", "{vlad.to_string()}" }
                            }
                            button {
                                class: "p-1 px-2 border rounded hover:bg-gray-100 text-xs font-mono",
                                // clipboard logic placeholder
                                "Copy"
                            }
                        }
                        if let Some(plog) = maybe_plog {
                            div {
                                class: "border-t pt-2 flex flex-col gap-1",
                                h4 { class: "font-semibold text-xs text-green-800", "Plog Details" }
                                PlogDisplay { plog: plog.log.clone() }
                            }
                        } else {
                            div {
                                class: "text-xs text-gray-500",
                                "No Plog available for this peer."
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PlogDisplay(plog: provenance_log::Log) -> Element {
    rsx! {
        div {
            class: "p-2 border rounded bg-neutral-100 text-green-800",
            h3 { class: "font-bold text-xs mb-1", "Plog Entries" }
            if plog.entries.is_empty() {
                p { class: "text-xs", "No entries in Plog." }
            } else {
                ul {
                    class: "list-disc pl-5 text-xs",
                    for (idx, maybe_verified) in plog.verify().enumerate() {
                        match maybe_verified {
                            Ok((_count, entry, _kvp)) => rsx! {
                                li {
                                    class: "mb-1",
                                    span { class: "font-mono text-xs mr-2", "Entry {idx}:" }
                                    DisplayEntry { entry: entry.clone() }
                                }
                            },
                            Err(e) => rsx! {
                                li {
                                    class: "mb-1 text-red-500",
                                    "Error verifying entry {idx}: {e}"
                                }
                            },
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
            class: "flex flex-col gap-1",
            for op in entry.ops() {
                DisplayOp { op: op.clone() }
            }
        }
    }
}

#[component]
fn DisplayOp(op: provenance_log::Op) -> Element {
    match op {
        provenance_log::Op::Noop(key) => rsx! {
            div {
                class: "flex gap-2 items-center text-xs",
                span { class: "font-mono text-gray-500", "No operation for" }
                span { class: "font-mono text-blue-700", "{key}" }
            }
        },
        provenance_log::Op::Delete(key) => rsx! {
            div {
                class: "flex gap-2 items-center text-xs",
                span { class: "font-mono text-red-500", "Deleted" }
                span { class: "font-mono text-red-800", "{key}" }
            }
        },
        provenance_log::Op::Update(key, value) => match value {
            provenance_log::Value::Nil => rsx! {
                div {
                    class: "flex gap-2 items-center text-xs",
                    span { class: "font-mono text-green-900", "{key}" }
                    span { class: "font-mono text-gray-400", "Nil" }
                }
            },
            provenance_log::Value::Str(s) => rsx! {
                div {
                    class: "flex gap-2 items-center text-xs",
                    span { class: "font-mono text-green-900", "{key}" }
                    span { class: "truncate font-mono text-green-600", "{s}" }
                }
            },
            provenance_log::Value::Data(data) => rsx! {
                div {
                    class: "flex gap-2 items-center text-xs",
                    span { class: "font-mono text-green-900", "{key}" }
                    span { class: "font-mono text-gray-700", "{data.len()} bytes" }
                }
            },
        },
    }
}
