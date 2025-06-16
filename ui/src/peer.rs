//! Peer component once a Wallet is available.
use bs_wallets::memory::InMemoryKeyManager;
use dioxus::prelude::*;

#[component]
pub fn Peer() -> Element {
    // use use_context_provider(|| use_signal(|| None::<InMemoryKeyManager<bs_wallets::Error>>));
    let key_manager = use_context::<Signal<Option<InMemoryKeyManager>>>();

    // assert that key manager is available, return a warning if not
    if key_manager().is_none() {
        return rsx! {
            div {
                class: "w-full h-full flex items-center justify-center",
                p { "No Key Manager available. Please initialize it first." }
            }
        };
    }

    rsx! {
        div {
            class: "w-full h-full flex flex-col items-center justify-center",
            h1 { "BetterSign Peer" }
            p { "This is where the Peer functionality will be implemented, CRUD stuff" }
            if key_manager.read().is_some() {
                p { "Key Manager is available" }
                // Here you can add more functionality related to the key manager
            } else {
                p { "No Key Manager available" }
            }
        }
    }
}
