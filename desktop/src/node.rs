//! Dioxus component that shows the node details, so others can connect to it.

use dioxus::prelude::*;

#[component]
pub fn DektopNode() -> Element {
    rsx! {
        div {
            id: "node",
            class: "text-green-500 font-mono",
            div {
                id: "links",
                "Some platform specific content"
            }
        }
    }
}
