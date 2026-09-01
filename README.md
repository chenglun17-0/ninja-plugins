# ninja-plugins

Ninja ADE 插件仓库。宿主默认零插件；这里放独立进程插件，只依赖 `ninja-protocol`，不链宿主、不链 libghostty。

装到 Ninja 的名字是 `~/.config/ninja/plugins/<name>` 里的文件名，和 `ninja.toml` 的 `enabled` 一致。

## preview（单文件编辑器）

⌘+click 终端里的文件路径，新开标签打开该文件，可以直接改。⌘S 保存；关标签会把未保存的草稿写回。同路径再次点击跳到对应行。Esc / ⌘W 关标签。

需要已经接线 `layer.open.surface` + `layer.msg` 的宿主。

```sh
./install-preview.sh
```

`~/.config/ninja/ninja.toml`：

```toml
[plugins]
enabled = ["preview"]
```

启用即拉起。⌘, 面板可开关。卸载：从 `enabled` 拿掉，删 `~/.config/ninja/plugins/preview`。完全退出 Ninja 再开。

## agent-restore

记住每个窗口/标签/分屏里正在跑的 CLI agent。关掉 Ninja 再开，宿主先按 `window-save-state` 把窗、标签和工作目录摆回去，本插件再往对应 PTY 打 resume 命令。

记录不按秒扫：前台进程/cwd 变了才推快照，退出前再推一次（对照 Orca 的 hook + quit-capture）。

第一步支持：

| 当时在跑 | 恢复时写入 |
| --- | --- |
| `pi` | `pi --session <id>` |
| `codex` | `codex resume <id>` |
| `claude` | `claude --resume <id>` |

关 Ninja 前已经退回 shell 的，不恢复。同一 Ninja 进程里插件热重载不会再打一遍。

```sh
./install-agent-restore.sh
```

需要一份已经接线 `pane.snapshot` / `pane.input` 的宿主。完全退出 Ninja 再开。

## py-pager（第二实现验证）

单文件 Python，不依赖 ninja-protocol：只按宿主文档（cookbook + golden JSON）
手写帧编解码，验证文档自足（PRODUCT 风险 4 的「非官方插件能否只靠文档完成
文件预览等价物」）。⌘+click 路径 → 只读标签页（html 表面，带行号与命中行
高亮）；不支持编辑。与 preview 同认领 path 时建议只启用其一（priority 50
< preview 的 100，两者同开时 preview 胜）。

```sh
./install-py-pager.sh   # 先跑协议级测试（假宿主驱动全周期），失败不装
```

需要 python3。代码在 `py-pager/py-pager`，协议测试在 `py-pager/test_pager.py`。
