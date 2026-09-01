#!/usr/bin/env bash
# 装py-pager 前先跑协议级测试（假宿主驱动全周期），失败不安装。
set -euo pipefail
cd "$(dirname "$0")"

python3 py-pager/test_pager.py

mkdir -p "${HOME}/.config/ninja/plugins"
cp -f py-pager/py-pager "${HOME}/.config/ninja/plugins/py-pager"
chmod +x "${HOME}/.config/ninja/plugins/py-pager"
echo "installed ${HOME}/.config/ninja/plugins/py-pager（python3 单文件，无构建）"
echo "启用：⌘, 面板把 py-pager 开 on（写回 ninja.toml），或手改："
echo '  [plugins]'
echo '  enabled = ["py-pager"]'
