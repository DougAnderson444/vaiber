//! Username and password based wallet Dioxus component.
use dioxus::prelude::*;
use seed_keeper_core::credentials::{Credentials, MinString, Wallet};

use crate::storage::StorageProvider;

const STORAGE_KEY: &str = "seed_keeper_encrypted_seed";
const MIN_LENGTH: usize = 8;

#[component]
pub fn WalletComponent() -> Element {
    let storage = use_context::<StorageProvider>();

    // State for the form
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error_message = use_signal(String::new);
    let mut success_message = use_signal(String::new);
    let mut wallet_signal = use_signal(|| None::<Wallet>);

    // Try to load existing seed from storage
    let mut encrypted_seed = use_signal(|| {
        if storage.exists(STORAGE_KEY) {
            storage.load(STORAGE_KEY).ok()
        } else {
            None
        }
    });

    let mut wallet_exists = use_signal(|| encrypted_seed().is_some());
    let mut is_loading_wallet = use_signal(|| false);

    let mut inputs_valid = use_signal(|| false);

    // Handle username input change
    let handle_username_change = move |evt: Event<FormData>| {
        username.set(evt.value().clone());
        inputs_valid.set(username().len() >= MIN_LENGTH && password().len() >= MIN_LENGTH);
    };

    // Handle password input change
    let handle_password_change = move |evt: Event<FormData>| {
        password.set(evt.value().clone());
        inputs_valid.set(username().len() >= MIN_LENGTH && password().len() >= MIN_LENGTH);
    };

    // Create a new wallet
    let mut create_wallet = {
        let storage = storage.clone();
        let error_message_clone = error_message;
        let success_message_clone = success_message;
        let encrypted_seed_clone = encrypted_seed;
        let wallet_exists_clone = wallet_exists;
        move || {
            let mut error_message = error_message_clone;
            let mut success_message = success_message_clone;
            let mut encrypted_seed = encrypted_seed_clone;
            let mut wallet_exists = wallet_exists_clone;

            error_message.set(String::new());
            success_message.set(String::new());

            // Validate inputs
            if username().is_empty() || password().is_empty() {
                error_message.set("Username and password cannot be empty".to_string());
                return;
            }

            // Enforce minimum length
            if username().len() < MIN_LENGTH || password().len() < MIN_LENGTH {
                error_message.set(format!(
                    "Username and password must be at least {MIN_LENGTH} characters"
                ));
                return;
            }

            // Create credentials
            let username_result = MinString::new(&username());
            let password_result = MinString::new(&password());

            if let (Ok(username_min), Ok(password_min)) = (username_result, password_result) {
                let credentials = Credentials {
                    username: username_min,
                    password: password_min,
                    encrypted_seed: None, // New wallet, no seed yet
                };

                // Create wallet
                match Wallet::new(credentials) {
                    Ok(wallet) => match wallet.encrypted_seed() {
                        Ok(seed) => {
                            let seed_clone = seed.clone();
                            encrypted_seed.set(Some(seed_clone.clone()));

                            // Save to storage
                            if let Err(err) = storage.save(STORAGE_KEY, &seed_clone) {
                                error_message.set(format!("Failed to save wallet: {err}"));
                                return;
                            }

                            wallet_exists.set(true);
                            success_message
                                .set("Wallet created and saved successfully".to_string());
                            wallet_signal.set(Some(wallet));

                            // Clear inputs
                            username.set(String::new());
                            password.set(String::new());
                        }
                        Err(err) => error_message.set(format!("Error encrypting seed: {err}")),
                    },
                    Err(err) => error_message.set(format!("Error creating wallet: {err}")),
                }
            } else {
                error_message.set("Invalid username or password".to_string());
            }
        }
    };

    // Load an existing wallet
    let mut load_wallet = move || {
        error_message.set(String::new());
        success_message.set(String::new());
        is_loading_wallet.set(true);

        // Validate inputs
        if username().is_empty() || password().is_empty() {
            error_message.set("Username and password cannot be empty".to_string());
            is_loading_wallet.set(false);
            return;
        }

        if let Some(stored_seed) = encrypted_seed() {
            // Create credentials with existing encrypted seed
            let username_result = MinString::new(&username());
            let password_result = MinString::new(&password());

            if let (Ok(username_min), Ok(password_min)) = (username_result, password_result) {
                let credentials = Credentials {
                    username: username_min,
                    password: password_min,
                    encrypted_seed: Some(stored_seed),
                };

                // Try to load wallet with provided credentials
                match Wallet::new(credentials) {
                    Ok(wallet) => {
                        success_message.set("Wallet loaded successfully".to_string());
                        wallet_signal.set(Some(wallet));
                    }
                    Err(err) => {
                        error_message.set(format!("Failed to load wallet: {err}. Please check your username and password."));
                    }
                }
            } else {
                error_message.set("Invalid username or password".to_string());
            }
        } else {
            error_message.set("No encrypted seed found. Please create a wallet first.".to_string());
        }

        is_loading_wallet.set(false);
    };

    // Reset wallet data
    let reset_wallet = {
        let storage = storage.clone();
        move |_| {
            // Clear storage
            if let Err(err) = storage.delete(STORAGE_KEY) {
                error_message.set(format!("Failed to clear wallet data: {err}"));
                return;
            }

            // Reset state
            encrypted_seed.set(None);
            wallet_exists.set(false);
            wallet_signal.set(None);
            username.set(String::new());
            password.set(String::new());
            error_message.set(String::new());
            success_message.set("Wallet data cleared successfully".to_string());
        }
    };

    // Lock wallet (just clears the wallet_signal without deleting storage)
    let lock_wallet = move |_| {
        wallet_signal.set(None);
        username.set(String::new());
        password.set(String::new());
        success_message.set("Wallet locked successfully".to_string());
    };

    let mut create_wallet_clone = create_wallet.clone();
    let handle_keydown = move |evt: Event<KeyboardData>| {
        if evt.key() == Key::Enter && inputs_valid() {
            if wallet_exists() {
                load_wallet();
            } else {
                create_wallet_clone();
            }
        }
    };

    // Get formatted encrypted seed for display
    let formatted_seed = encrypted_seed()
        .as_ref()
        .map(|bytes| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());

    // Prepare UI components based on state
    let error_ui = (!error_message().is_empty()).then(|| {
        rsx! {
            div {
                class: "p-3 bg-red-100 border border-red-300 rounded-md text-red-700 text-sm",
                "{error_message()}"
            }
        }
    });

    let success_ui = (!success_message().is_empty()).then(|| {
        rsx! {
            div {
                class: "p-3 bg-green-100 border border-green-300 rounded-md text-green-700 text-sm",
                "{success_message()}"
            }
        }
    });

    let seed_ui = formatted_seed.as_ref().map(|seed_str| {
        rsx! {
            div {
                class: "mt-4 p-3 bg-gray-100 rounded-md",
                h3 { class: "text-sm font-bold text-gray-700 mb-1", "Encrypted Seed" }
                p { class: "text-xs break-all text-gray-600", "{seed_str}" }
                div {
                    class: "mt-2 text-xs text-gray-500",
                    "Store this encrypted seed securely to restore your wallet later"
                }
            }
        }
    });

    let action_button = if wallet_exists() {
        rsx! {
            button {
                class: "w-full bg-blue-500 text-white py-2 px-4 rounded-md hover:bg-blue-600 transition",
                r#type: "button",
                onclick: move |_| load_wallet(),
                disabled: is_loading_wallet(),
                if is_loading_wallet() { "Loading Wallet..." } else { "Access Wallet" }
            }
        }
    } else {
        rsx! {
            button {
                class: "w-full bg-green-500 text-white py-2 px-4 rounded-md hover:bg-green-600 transition disabled:bg-gray-400",
                r#type: "button",
                onclick: move |_| create_wallet(),
                disabled: !inputs_valid(),
                if inputs_valid() { "Create New Wallet" } else { "Use longer username/password" }
            }
        }
    };

    // New active wallet UI component
    let active_wallet_ui = rsx! {
        div {
            class: "space-y-4",
            div {
                class: "p-4 bg-green-50 border border-green-200 rounded-lg",
                div {
                    class: "flex items-center justify-center mb-3",
                    svg {
                        class: "w-8 h-8 text-green-600",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "currentColor",
                        path {
                            d: "M7 8a3 3 0 100-6 3 3 0 000 6zm7-5h-4v2h4V3zm3 5h-4V6h4v2zm-4 8v7h4v-7h-4zm5-12h-4v2h4V4zm-4 16h4v-2h-4v2zM4 4v16h6v-2H6V6h4V4H4z"
                        }
                    }
                }
                h3 {
                    class: "text-xl font-bold text-center text-green-800",
                    "Wallet Active"
                }
                p {
                    class: "text-center text-green-600 mt-2",
                    "Your wallet is unlocked and ready to use"
                }

                div {
                    class: "mt-4 flex items-center justify-center space-x-2",
                    button {
                        class: "bg-green-600 text-white py-2 px-4 rounded-md hover:bg-green-700 transition",
                        r#type: "button",
                        "View Key Details"
                    }
                    button {
                        class: "bg-white border border-green-600 text-green-600 py-2 px-4 rounded-md hover:bg-green-50 transition",
                        r#type: "button",
                        onclick: lock_wallet,
                        "Lock Wallet"
                    }
                }
            }

            // If wallet is accessible, still show the seed for reference
            {seed_ui}

            // Success message
            {success_ui.clone()}
        }
    };

    // Main container with conditional content based on wallet accessibility
    rsx! {
        div {
            id: "wallet",
            class: "text-green-500 max-w-md mx-auto my-8 p-6 bg-white rounded-lg shadow-md",

            div { class: "mb-6",
                h2 { class: "text-2xl font-bold",
                    if wallet_signal.read().is_some() {
                        "Wallet Dashboard"
                    } else if wallet_exists() {
                        "Access Your Wallet"
                    } else {
                        "Create New Wallet"
                    }
                }
                p { class: "text-gray-600",
                    if wallet_signal.read().is_some() {
                        "Your secure wallet is ready to use"
                    } else if wallet_exists() {
                        "Enter your username and password to access your wallet"
                    } else {
                        "Create a new secure wallet with a username and password using Argon2"
                    }
                }
            }

            // Show different UI based on wallet signal
            if wallet_signal.read().is_some() {
                {active_wallet_ui}
            } else {
                div { class: "space-y-4",
                    div { class: "space-y-2",
                        label { class: "block text-sm font-medium text-gray-700", "Username" }
                        input {
                            class: "w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-green-500",
                            r#type: "text",
                            value: "{username}",
                            oninput: handle_username_change,
                            onkeydown: handle_keydown.clone(),
                            placeholder: format!("Minimum {MIN_LENGTH} characters")
                        }
                    }

                    div { class: "space-y-2",
                        label { class: "block text-sm font-medium text-gray-700", "Password" }
                        input {
                            class: "w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-green-500",
                            r#type: "password",
                            value: "{password}",
                            oninput: handle_password_change,
                            onkeydown: handle_keydown,
                            placeholder: format!("Minimum {MIN_LENGTH} characters")
                        }
                    }

                    // Main action button (create or access)
                    {action_button}

                    // Reset button (only show if wallet exists but not accessible)
                    if wallet_exists() {
                        button {
                            class: "w-full mt-2 bg-red-100 text-red-700 py-2 px-4 rounded-md hover:bg-red-200 transition",
                            r#type: "button",
                            onclick: reset_wallet,
                            "Reset Wallet"
                        }
                    }

                    // Error message
                    {error_ui}

                    // Success message
                    {success_ui}
                }
            }

            div { id: "links", class: "mt-6 text-center text-sm text-gray-600",
                a {
                    href: "#",
                    class: "text-green-600 hover:underline",
                    "Learn about wallet security"
                }
            }
        }
    }
}
