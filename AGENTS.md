# Agent notes — ninja-plugins

This repository is **official / example Ninja plugins**. Each plugin is a separate process. It may depend on `ninja-protocol` only — never on the host crate `ninja`, never on `ghostty-sys`.

The host lives in [chenglun17-0/ninja](https://github.com/chenglun17-0/ninja). New product features belong here if they can be built from existing ADE primitives (`hit`, `layer`, `input`, `spawn`, `config`, `theme`, `pane`). If a change requires a host diff, that change is a kernel primitive — do it in the host repo first, with no plugin nouns in the protocol.

## Do

- Talk to the host only over length-prefixed JSON frames (`NINJA_ADE_SOCK`).
- Keep filesystem I/O local to the plugin (read/write the files it opened). JS in an HTML layer must not choose arbitrary paths.
- Treat `layer.msg` `name`/`body` as plugin-private. The host forwards them opaquely.

## Don't

- Don't add `file.*` / `save` / `lsp` types to the protocol from this repo.
- Don't assume one layer per process if the host already allows many tabs.
- Don't commit plugin binaries, secrets, or `target/`.

## Git commits

Write **commit messages in English**.

- Imperative subject, ~50–72 characters: `feat: …`, `fix: …`, `docs: …`.
- Body (optional) explains *why*. Wrap at 72.
- Keep protocol lockstep with the host copy of `ninja-protocol` when you change the wire format.

Examples:

```text
feat: open claimed paths as an editable HTML tab

Cmd-S and layer close write the last draft. Same path reuses the tab.
```

```text
fix: strip file:// from OSC-7 cwd before joining relative hits
```
