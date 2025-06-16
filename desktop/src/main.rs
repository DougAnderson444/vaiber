//! DESKTOP
mod storage;

use dioxus::prelude::*;

use ui::{Hero, StorageProvider};

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Build cool things ✌️
    let storage = storage::DesktopStorage;
    let storage_provider = StorageProvider::new(storage);

    // provide storgae in context for all child elements
    use_context_provider(|| storage_provider);

    rsx! {
        // Global app resources
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        Hero {}

    }
}
