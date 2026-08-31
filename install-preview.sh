#!/usr/bin/env bash
# 编 preview 并装进本机 Ninja（~/.config/ninja/plugins/preview）。
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release -p ninja-preview
mkdir -p "${HOME}/.config/ninja/plugins"
cp -f target/release/ninja-preview "${HOME}/.config/ninja/plugins/preview"
chmod +x "${HOME}/.config/ninja/plugins/preview"
TOML="${HOME}/.config/ninja/ninja.toml"
if [[ ! -f "$TOML" ]]; then
  mkdir -p "$(dirname "$TOML")"
  printf '[plugins]\nenabled = ["preview"]\n' >"$TOML"
elif ! grep -q 'enabled' "$TOML"; then
  printf '\n[plugins]\nenabled = ["preview"]\n' >>"$TOML"
fi
echo "installed ${HOME}/.config/ninja/plugins/preview"
echo "ninja.toml: $TOML"
