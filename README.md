# ninja-plugins

Ninja ADE 插件仓库。宿主默认零插件；这里放独立进程插件，只依赖 `ninja-protocol`，不链宿主、不链 libghostty。

装到 Ninja 的名字是 `~/.config/ninja/plugins/<name>` 里的文件名，和 `ninja.toml` 的 `enabled` 一致。

## preview

点终端里的文件路径，在层里看文本/代码。

```sh
cargo build --release -p ninja-preview
mkdir -p ~/.config/ninja/plugins
cp target/release/ninja-preview ~/.config/ninja/plugins/preview
```

`~/.config/ninja/ninja.toml`：

```toml
[plugins]
enabled = ["preview"]
```

启用即拉起。⌘, 面板可开关。卸载：从 `enabled` 拿掉，删 `~/.config/ninja/plugins/preview`。
