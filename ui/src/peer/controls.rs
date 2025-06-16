//! This Dioxus component allows users to interact with the Plog controls of a peer.
//!
//! - It shows whether or not a peer has a plog
//! - It provides a button to create a plog if it doesn't exist.
//! - If it does exist, it displays the plog's vlad as a string for user informatio, with a
//!   clipboard button to copy the vlad to the clipboard.
//! -
use bs_peer::peer::DefaultBsPeer;
use bs_peer::utils::create_default_scripts;
use dioxus::prelude::*;

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
        }
    }
}
