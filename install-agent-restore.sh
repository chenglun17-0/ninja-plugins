#!/usr/bin/env bash
# 编 agent-restore 并装进本机 Ninja（~/.config/ninja/plugins/agent-restore）。
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release -p ninja-agent-restore
mkdir -p "${HOME}/.config/ninja/plugins"
cp -f target/release/ninja-agent-restore "${HOME}/.config/ninja/plugins/agent-restore"
chmod +x "${HOME}/.config/ninja/plugins/agent-restore"
TOML="${HOME}/.config/ninja/ninja.toml"
if [[ ! -f "$TOML" ]]; then
  mkdir -p "$(dirname "$TOML")"
  printf '[plugins]\nenabled = ["agent-restore"]\n' >"$TOML"
elif grep -q 'agent-restore' "$TOML"; then
  :
elif grep -q 'enabled = \[\]' "$TOML"; then
  python3 - "$TOML" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
text = p.read_text()
text = text.replace("enabled = []", 'enabled = ["agent-restore"]', 1)
p.write_text(text)
PY
elif grep -q 'enabled = \[' "$TOML"; then
  python3 - "$TOML" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
text = p.read_text()
text = text.replace("enabled = [", 'enabled = ["agent-restore", ', 1)
p.write_text(text)
PY
else
  printf '\n[plugins]\nenabled = ["agent-restore"]\n' >>"$TOML"
fi
echo "installed ${HOME}/.config/ninja/plugins/agent-restore"
echo "ninja.toml: $TOML"
echo "完全退出 Ninja 再开。宿主需带 pane.snapshot / pane.input（新编过的 Ninja）。"
