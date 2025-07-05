build-web:
  just web/build
  mkdir -p docs
  mv web/docs/public/* docs
  cp docs/index.html docs/404.html
  # Add and commit with git 
  git add docs
  git commit -m "Update docs" || true

serve-web:
  RUSTFLAGS='--cfg getrandom_backend="wasm_js"' dx serve --package web --platform web

css:
  tailwindcss -i ./input.css -o ./ui/assets/tailwind.css --watch &
  tailwindcss -i ./input.css -o ./desktop/assets/tailwind.css --watch

# Desktop Version 
serve-desktop:
  dx serve --package desktop --platform desktop

# This is for the second desktop version, so 2 kad nodes can run 
# and connect with each other with different identities and certhashes
serve-second-desktop:
  #!/usr/bin/env bash
  # Exit if any command in a pipeline fails
  set -euo pipefail
  # This is for the second desktop version, if needed
  export DIOXUS_IDENTITY=second && dx serve --package desktop --platform desktop
