//! 单文件编辑器页面：自包含 HTML + 内联 JS。
//! 打字/IME/选区在 WebKit；存盘经 `layer.msg`（name 是插件与页面的约定）。

use std::path::Path;

pub fn document(path: &Path, content: &str, line: Option<u32>, col: Option<u32>) -> String {
    let title = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");
    let path_s = path.to_string_lossy();
    let start_line = line.unwrap_or(1).max(1);
    let start_col = col.unwrap_or(1).max(1);
    format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline';">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  :root {{
    color-scheme: dark;
    --bg: #16181d;
    --fg: #cfd3db;
    --muted: #7a8190;
    --rule: #2c333e;
    --amber: #c7925b;
    --blue: #82aaff;
    --flag: transparent;
  }}
  html, body {{
    margin: 0; height: 100%;
    background: var(--bg); color: var(--fg);
  }}
  body {{
    display: flex; flex-direction: column;
    font: 12px/1.4 "SF Mono", Menlo, ui-monospace, monospace;
  }}
  #bar {{
    flex: 0 0 auto;
    display: flex; align-items: baseline; gap: 1.25rem;
    padding: 7px 14px 6px;
    border-bottom: 1px solid var(--rule);
    color: var(--muted);
    user-select: none;
  }}
  #path {{
    color: var(--fg);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    min-width: 0; flex: 1;
  }}
  #flag {{
    color: var(--amber);
    min-width: 0.7em;
    font-weight: 700;
  }}
  #pos, #hint {{ letter-spacing: 0.04em; }}
  #hint {{ margin-left: auto; }}
  #ed {{
    flex: 1 1 auto;
    width: 100%;
    border: 0; outline: none; resize: none;
    padding: 12px 16px 24px 14px;
    background: var(--bg);
    color: var(--fg);
    caret-color: var(--amber);
    font: 13px/1.55 "SF Mono", Menlo, ui-monospace, monospace;
    tab-size: 4;
    box-shadow: inset 2px 0 0 var(--flag);
  }}
  body.dirty #ed {{ --flag: var(--amber); }}
  body.dirty #flag::before {{ content: "+"; }}
</style>
</head>
<body>
<div id="bar">
  <span id="path">{path_esc}</span>
  <span id="flag"></span>
  <span id="pos">1:1</span>
  <span id="hint">⌘S</span>
</div>
<textarea id="ed" spellcheck="false" autocapitalize="off" autocomplete="off" autocorrect="off">{body}</textarea>
<script>
(function() {{
  const ed = document.getElementById("ed");
  const pos = document.getElementById("pos");
  const START_LINE = {start_line};
  const START_COL = {start_col};
  let dirty = false;
  let draftTimer = 0;

  function post(name, body) {{
    try {{
      window.webkit.messageHandlers.layer.postMessage({{name: name, body: body}});
    }} catch (e) {{}}
  }}
  function caret() {{
    const v = ed.value;
    const i = ed.selectionStart || 0;
    let line = 1, col = 1;
    for (let k = 0; k < i; k++) {{
      if (v.charCodeAt(k) === 10) {{ line++; col = 1; }}
      else col++;
    }}
    return [line, col];
  }}
  function paint() {{
    const [l, c] = caret();
    pos.textContent = l + ":" + c;
    document.body.classList.toggle("dirty", dirty);
  }}
  function gotoLine(line, col) {{
    const v = ed.value;
    let i = 0, l = 1;
    const target = Math.max(1, line|0);
    while (l < target && i < v.length) {{
      if (v.charCodeAt(i) === 10) l++;
      i++;
    }}
    i += Math.max(0, (col|0) - 1);
    if (i > v.length) i = v.length;
    ed.focus();
    ed.setSelectionRange(i, i);
    const lh = 20.15;
    ed.scrollTop = Math.max(0, (target - 3) * lh);
    paint();
  }}
  function markDirty() {{
    if (!dirty) dirty = true;
    paint();
    clearTimeout(draftTimer);
    draftTimer = setTimeout(function() {{ post("draft", ed.value); }}, 150);
  }}
  function save() {{
    clearTimeout(draftTimer);
    post("save", ed.value);
  }}
  ed.addEventListener("input", markDirty);
  ed.addEventListener("click", paint);
  ed.addEventListener("keyup", paint);
  ed.addEventListener("select", paint);
  document.addEventListener("keydown", function(e) {{
    if ((e.metaKey || e.ctrlKey) && (e.key === "s" || e.key === "S")) {{
      e.preventDefault();
      save();
      return;
    }}
    if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {{
      e.preventDefault();
      const a = ed.selectionStart, b = ed.selectionEnd;
      ed.setRangeText("\t", a, b, "end");
      markDirty();
    }}
  }});
  window.addEventListener("pagehide", function() {{
    if (dirty) post("draft", ed.value);
  }});
  window.addEventListener("layer-msg", function(e) {{
    const d = (e && e.detail) || {{}};
    if (d.name === "goto") {{
      const parts = String(d.body || "").split(":");
      gotoLine(parseInt(parts[0] || "1", 10), parseInt(parts[1] || "1", 10));
    }} else if (d.name === "saved") {{
      dirty = false;
      paint();
    }}
  }});
  gotoLine(START_LINE, START_COL);
}})();
</script>
</body>
</html>
"##,
        title = html_escape(title),
        path_esc = html_escape(&path_s),
        body = html_escape(content),
        start_line = start_line,
        start_col = start_col,
    )
}

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn textarea_escapes_close_tag() {
        let html = document(
            Path::new("a.rs"),
            "fn x() {}\n</textarea><script>",
            None,
            None,
        );
        assert!(html.contains("&lt;/textarea&gt;"), "{html}");
        assert!(html.contains("script-src 'unsafe-inline'"));
        assert!(html.contains("webkit.messageHandlers.layer"));
    }

    #[test]
    fn start_line_is_baked() {
        let html = document(Path::new("a.rs"), "a\nb\nc\n", Some(3), Some(1));
        assert!(html.contains("const START_LINE = 3;"), "{html}");
        assert!(html.contains("a.rs"));
    }
}
