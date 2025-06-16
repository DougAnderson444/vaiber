use dioxus::prelude::*;

use crate::{peer::Peer, WalletComponent};

const PEERPIPER_P_SVG: Asset = asset!("/assets/p.svg");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[component]
pub fn Hero() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        div {
            id: "hero",
            class: "text-green-500",
            div { id: "links",
                WalletComponent { content: rsx! { Peer { } } }
            }
        }
    }
}
