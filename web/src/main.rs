mod storage;

use dioxus::prelude::*;

use ui::{Hero, StorageProvider};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Build cool things ✌️
    let storage = storage::WebStorage::new();
    let storage_provider = StorageProvider::new(storage);

    // provide storgae in context for all child elements
    use_context_provider(|| storage_provider);

    rsx! {
        // Global app resources
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        Hero {}

    }
}
