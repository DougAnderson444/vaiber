//! DESKTOP
mod error;
mod node;
mod storage;

use error::Error;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use ui::{Hero, StorageProvider};

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::LaunchBuilder::new()
        .with_cfg(desktop! {
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("PeerPiper vaiber")
                    .with_inner_size(LogicalSize::new(700.0, 900.0)),
            )
        })
        .launch(App)
}

#[component]
fn App() -> Element {
    // Build cool things ✌️
    let storage = storage::DesktopStorage::new().unwrap();
    let storage_provider = StorageProvider::new(storage.clone());

    // provide storage in context for all child elements
    use_context_provider(|| storage_provider);

    rsx! {
        // Global app resources
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        Hero { platform_content: rsx! { node::DektopNode { } }, base_path: storage.dir() }
    }
}
