use dioxus::prelude::*;

use crate::WalletComponent;

const HEADER_SVG: Asset = asset!("/assets/header.svg");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[component]
pub fn Hero() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        div {
            id: "hero",
            class: "text-green-500",
            div { id: "links",
                a { href: "https://dioxuslabs.com/learn/0.6/", "Summer is cool" }
                WalletComponent {  }
            }
        }
    }
}
