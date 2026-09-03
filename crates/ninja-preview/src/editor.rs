//! 单文件页面：Markdown 渲染；代码/文本可编辑并做语法高亮。
//! 存盘经 `layer.msg`（name 是插件与页面的约定）。

use std::path::Path;

use pulldown_cmark::{Options, Parser, html};

pub fn document(path: &Path, content: &str, line: Option<u32>, col: Option<u32>) -> String {
    if is_markdown(path) {
        markdown_page(path, content)
    } else {
        editor_page(path, content, line, col)
    }
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        ext_of(path).as_deref(),
        Some("md" | "markdown" | "mdown" | "mdwn")
    )
}

fn ext_of(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    if name.eq_ignore_ascii_case("Makefile")
        || name.eq_ignore_ascii_case("GNUmakefile")
        || name.eq_ignore_ascii_case("Dockerfile")
    {
        return Some(name.to_ascii_lowercase());
    }
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
}

fn language_of(path: &Path) -> &'static str {
    match ext_of(path).as_deref() {
        Some("rs") => "rs",
        Some("py" | "pyi") => "py",
        Some("js" | "mjs" | "cjs") => "js",
        Some("ts") => "ts",
        Some("tsx") => "tsx",
        Some("jsx") => "jsx",
        Some("go") => "go",
        Some("c" | "h") => "c",
        Some("cc" | "cpp" | "cxx" | "hpp" | "hh") => "cpp",
        Some("java") => "java",
        Some("kt" | "kts") => "kt",
        Some("swift") => "swift",
        Some("rb") => "rb",
        Some("php") => "php",
        Some("cs") => "cs",
        Some("lua") => "lua",
        Some("zig") => "zig",
        Some("sql") => "sql",
        Some("sh" | "bash" | "zsh" | "makefile" | "gnumakefile") => "sh",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("yaml" | "yml") => "yaml",
        Some("html" | "htm" | "xml") => "html",
        Some("css" | "scss") => "css",
        Some("proto") => "proto",
        Some("dockerfile") => "docker",
        _ => "txt",
    }
}

fn markdown_page(path: &Path, content: &str) -> String {
    let title = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");
    let path_s = path.to_string_lossy();
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(content, opts);
    let mut body = String::new();
    html::push_html(&mut body, parser);
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
    --bg: #16181d; --fg: #cfd3db; --muted: #7a8190; --rule: #2c333e;
    --amber: #c7925b; --blue: #82aaff; --green: #c3e88d; --red: #f07178;
    --code-bg: #0f1115;
  }}
  html, body {{ margin: 0; height: 100%; background: var(--bg); color: var(--fg); }}
  body {{ display: flex; flex-direction: column; font: 14px/1.55 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
  #bar {{
    flex: 0 0 auto; display: flex; align-items: baseline; gap: 1.25rem;
    padding: 7px 14px 6px; border-bottom: 1px solid var(--rule); color: var(--muted);
    font: 12px/1.4 "SF Mono", Menlo, ui-monospace, monospace; user-select: none;
  }}
  #path {{ color: var(--fg); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1; }}
  #lang {{ letter-spacing: 0.04em; text-transform: uppercase; }}
  article {{
    flex: 1 1 auto; overflow: auto; padding: 28px 36px 48px;
    max-width: 52rem; width: 100%; box-sizing: border-box; margin: 0 auto;
  }}
  article h1, article h2, article h3 {{ color: #e8eaef; font-weight: 650; line-height: 1.25; }}
  article h1 {{ font-size: 1.7rem; margin: 0 0 0.8em; padding-bottom: 0.35em; border-bottom: 1px solid var(--rule); }}
  article h2 {{ font-size: 1.3rem; margin: 1.4em 0 0.6em; }}
  article h3 {{ font-size: 1.1rem; margin: 1.2em 0 0.5em; }}
  article p, article li {{ color: var(--fg); }}
  article a {{ color: var(--blue); text-decoration: none; }}
  article a:hover {{ text-decoration: underline; }}
  article hr {{ border: 0; border-top: 1px solid var(--rule); margin: 2em 0; }}
  article blockquote {{
    margin: 1em 0; padding: 0.2em 1em; border-left: 3px solid var(--amber);
    color: var(--muted);
  }}
  article code {{
    font: 85%/1.45 "SF Mono", Menlo, ui-monospace, monospace;
    background: var(--code-bg); padding: 0.12em 0.35em; border-radius: 4px;
  }}
  article pre {{
    background: var(--code-bg); border: 1px solid var(--rule); border-radius: 8px;
    padding: 12px 14px; overflow: auto;
  }}
  article pre code {{ background: none; padding: 0; font-size: 12.5px; line-height: 1.55; }}
  article table {{ border-collapse: collapse; margin: 1em 0; width: 100%; }}
  article th, article td {{ border: 1px solid var(--rule); padding: 6px 10px; text-align: left; }}
  article th {{ background: var(--code-bg); }}
  .k {{ color: var(--blue); }} .s {{ color: var(--green); }} .c {{ color: var(--muted); font-style: italic; }}
  .n {{ color: var(--amber); }}
</style>
</head>
<body>
<div id="bar">
  <span id="path">{path_esc}</span>
  <span id="lang">md</span>
</div>
<article>{body}</article>
<script>
{highlighter}
(function() {{
  document.querySelectorAll("pre code").forEach(function(el) {{
    var cls = el.className || "";
    var m = cls.match(/language-([a-z0-9+#]+)/i);
    var lang = m ? m[1].toLowerCase() : "txt";
    if (lang === "rust") lang = "rs";
    if (lang === "python") lang = "py";
    if (lang === "javascript") lang = "js";
    if (lang === "typescript") lang = "ts";
    if (lang === "shell" || lang === "bash") lang = "sh";
    el.innerHTML = highlight(el.textContent, lang);
  }});
}})();
</script>
</body>
</html>
"##,
        title = html_escape(title),
        path_esc = html_escape(&path_s),
        body = body,
        highlighter = HIGHLIGHTER_JS,
    )
}

fn editor_page(path: &Path, content: &str, line: Option<u32>, col: Option<u32>) -> String {
    let title = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");
    let path_s = path.to_string_lossy();
    let lang = language_of(path);
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
    --bg: #16181d; --fg: #cfd3db; --muted: #7a8190; --rule: #2c333e;
    --amber: #c7925b; --blue: #82aaff; --green: #c3e88d; --flag: transparent;
  }}
  html, body {{ margin: 0; height: 100%; background: var(--bg); color: var(--fg); }}
  body {{ display: flex; flex-direction: column; font: 12px/1.4 "SF Mono", Menlo, ui-monospace, monospace; }}
  #bar {{
    flex: 0 0 auto; display: flex; align-items: baseline; gap: 1.25rem;
    padding: 7px 14px 6px; border-bottom: 1px solid var(--rule); color: var(--muted);
    user-select: none;
  }}
  #path {{ color: var(--fg); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1; }}
  #flag {{ color: var(--amber); min-width: 0.7em; font-weight: 700; }}
  #pos, #hint, #lang {{ letter-spacing: 0.04em; }}
  #lang {{ text-transform: uppercase; }}
  #hint {{ margin-left: auto; }}
  #wrap {{ position: relative; flex: 1 1 auto; min-height: 0; }}
  #hi, #ed {{
    position: absolute; inset: 0; margin: 0; border: 0; outline: none;
    padding: 12px 16px 24px 14px;
    font: 13px/1.55 "SF Mono", Menlo, ui-monospace, monospace;
    tab-size: 4; white-space: pre; overflow: auto;
    box-sizing: border-box; width: 100%; height: 100%;
  }}
  #hi {{
    color: var(--fg); background: var(--bg); pointer-events: none;
    box-shadow: inset 2px 0 0 var(--flag);
  }}
  #ed {{
    resize: none; background: transparent; color: transparent;
    caret-color: var(--amber); z-index: 1;
  }}
  body.dirty #hi {{ --flag: var(--amber); }}
  body.dirty #flag::before {{ content: "+"; }}
  .k {{ color: var(--blue); }} .s {{ color: var(--green); }} .c {{ color: var(--muted); font-style: italic; }}
  .n {{ color: var(--amber); }}
</style>
</head>
<body>
<div id="bar">
  <span id="path">{path_esc}</span>
  <span id="flag"></span>
  <span id="lang">{lang}</span>
  <span id="pos">1:1</span>
  <span id="hint">⌘S</span>
</div>
<div id="wrap">
<pre id="hi" aria-hidden="true"></pre>
<textarea id="ed" spellcheck="false" autocapitalize="off" autocomplete="off" autocorrect="off">{body}</textarea>
</div>
<script>
{highlighter}
(function() {{
  const ed = document.getElementById("ed");
  const hi = document.getElementById("hi");
  const pos = document.getElementById("pos");
  const LANG = "{lang}";
  const START_LINE = {start_line};
  const START_COL = {start_col};
  let dirty = false;
  let draftTimer = 0;

  function post(name, body) {{
    try {{ window.webkit.messageHandlers.layer.postMessage({{name: name, body: body}}); }}
    catch (e) {{}}
  }}
  function paintHi() {{ hi.innerHTML = highlight(ed.value, LANG) + "\\n"; }}
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
  function syncScroll() {{ hi.scrollTop = ed.scrollTop; hi.scrollLeft = ed.scrollLeft; }}
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
    syncScroll();
    paint();
  }}
  function markDirty() {{
    if (!dirty) dirty = true;
    paintHi();
    paint();
    clearTimeout(draftTimer);
    draftTimer = setTimeout(function() {{ post("draft", ed.value); }}, 150);
  }}
  function save() {{
    clearTimeout(draftTimer);
    post("save", ed.value);
  }}
  ed.addEventListener("input", markDirty);
  ed.addEventListener("scroll", syncScroll);
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
      ed.setRangeText("\\t", a, b, "end");
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
  paintHi();
  gotoLine(START_LINE, START_COL);
}})();
</script>
</body>
</html>
"##,
        title = html_escape(title),
        path_esc = html_escape(&path_s),
        body = html_escape(content),
        lang = html_escape(lang),
        start_line = start_line,
        start_col = start_col,
        highlighter = HIGHLIGHTER_JS,
    )
}

const HIGHLIGHTER_JS: &str = r###"
function esc(s) {
  return s.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;");
}
const KW = {
  rs: /\b(as|async|await|break|const|continue|crate|dyn|else|enum|extern|false|fn|for|if|impl|in|let|loop|match|mod|move|mut|pub|ref|return|self|Self|static|struct|super|trait|true|type|unsafe|use|where|while)\b/g,
  py: /\b(and|as|assert|async|await|break|class|continue|def|del|elif|else|except|False|finally|for|from|global|if|import|in|is|lambda|None|nonlocal|not|or|pass|raise|return|True|try|while|with|yield)\b/g,
  js: /\b(async|await|break|case|catch|class|const|continue|debugger|default|delete|do|else|export|extends|false|finally|for|function|if|import|in|instanceof|let|new|null|of|return|static|super|switch|this|throw|true|try|typeof|undefined|var|void|while|yield)\b/g,
  ts: /\b(abstract|as|async|await|break|case|catch|class|const|continue|debugger|default|delete|do|else|enum|export|extends|false|finally|for|from|function|if|implements|import|in|instanceof|interface|let|new|null|of|private|protected|public|readonly|return|static|super|switch|this|throw|true|try|type|typeof|undefined|var|void|while|yield)\b/g,
  go: /\b(break|case|chan|const|continue|default|defer|else|fallthrough|for|func|go|goto|if|import|interface|map|package|range|return|select|struct|switch|type|var)\b/g,
  c: /\b(auto|break|case|char|const|continue|default|do|double|else|enum|extern|float|for|goto|if|inline|int|long|register|return|short|signed|sizeof|static|struct|switch|typedef|union|unsigned|void|volatile|while)\b/g,
  cpp: /\b(alignas|alignof|auto|bool|break|case|catch|char|class|const|constexpr|continue|decltype|default|delete|do|double|else|enum|explicit|export|extern|false|float|for|friend|goto|if|inline|int|long|mutable|namespace|new|noexcept|nullptr|operator|private|protected|public|return|short|signed|sizeof|static|static_cast|struct|switch|template|this|throw|true|try|typedef|typename|union|unsigned|using|virtual|void|volatile|while)\b/g,
  java: /\b(abstract|assert|boolean|break|byte|case|catch|char|class|const|continue|default|do|double|else|enum|extends|false|final|finally|float|for|goto|if|implements|import|instanceof|int|interface|long|native|new|null|package|private|protected|public|return|short|static|strictfp|super|switch|synchronized|this|throw|throws|transient|true|try|void|volatile|while)\b/g,
  kt: /\b(abstract|actual|as|break|by|catch|class|companion|const|continue|crossinline|data|do|else|enum|expect|false|final|finally|for|fun|if|in|infix|init|inline|inner|interface|internal|is|lateinit|noinline|null|object|open|operator|out|override|package|private|protected|public|reified|return|sealed|super|suspend|this|throw|true|try|typealias|val|var|when|where|while)\b/g,
  swift: /\b(as|associatedtype|break|case|catch|class|continue|default|defer|deinit|do|else|enum|extension|fallthrough|false|fileprivate|for|func|guard|if|import|in|init|inout|internal|is|let|nil|open|operator|override|private|protocol|public|repeat|return|self|static|struct|subscript|super|switch|throw|throws|true|try|typealias|var|where|while)\b/g,
  rb: /\b(alias|and|begin|break|case|class|def|defined|do|else|elsif|end|ensure|false|for|if|in|module|next|nil|not|or|redo|rescue|retry|return|self|super|then|true|undef|unless|until|when|while|yield)\b/g,
  php: /\b(abstract|and|array|as|break|case|catch|class|clone|const|continue|declare|default|do|echo|else|elseif|empty|enddeclare|endfor|endforeach|endif|endswitch|endwhile|extends|final|finally|fn|for|foreach|function|global|goto|if|implements|include|include_once|instanceof|interface|isset|list|match|namespace|new|or|private|protected|public|require|require_once|return|static|switch|throw|trait|try|unset|use|var|while|xor|yield)\b/g,
  cs: /\b(abstract|as|base|bool|break|byte|case|catch|char|checked|class|const|continue|decimal|default|delegate|do|double|else|enum|event|explicit|extern|false|finally|fixed|float|for|foreach|goto|if|implicit|in|int|interface|internal|is|lock|long|namespace|new|null|object|operator|out|override|params|private|protected|public|readonly|ref|return|sbyte|sealed|short|sizeof|stackalloc|static|string|struct|switch|this|throw|true|try|typeof|uint|ulong|unchecked|unsafe|ushort|using|virtual|void|volatile|while)\b/g,
  lua: /\b(and|break|do|else|elseif|end|false|for|function|goto|if|in|local|nil|not|or|repeat|return|then|true|until|while)\b/g,
  zig: /\b(align|allowzero|and|anyframe|anytype|asm|async|await|break|cancel|catch|comptime|const|continue|defer|else|enum|errdefer|error|export|extern|false|fn|for|if|inline|linksection|noalias|nosuspend|null|or|orelse|packed|promise|pub|resume|return|struct|suspend|switch|test|threadlocal|true|try|undefined|union|unreachable|usingnamespace|var|volatile|while)\b/g,
  sql: /\b(ADD|ALL|ALTER|AND|AS|ASC|BETWEEN|BY|CASE|CHECK|COLUMN|CONSTRAINT|CREATE|DATABASE|DEFAULT|DELETE|DESC|DISTINCT|DROP|ELSE|END|EXISTS|FOREIGN|FROM|FULL|GROUP|HAVING|IN|INDEX|INNER|INSERT|INTO|IS|JOIN|KEY|LEFT|LIKE|LIMIT|NOT|NULL|ON|OR|ORDER|OUTER|PRIMARY|REFERENCES|RIGHT|SELECT|SET|TABLE|THEN|UNION|UNIQUE|UPDATE|VALUES|WHERE)\b/gi,
  sh: /\b(alias|bg|bind|break|builtin|caller|case|cd|command|continue|declare|do|done|echo|elif|else|esac|eval|exec|exit|export|false|fi|for|function|if|in|jobs|kill|let|local|printf|pwd|read|readonly|return|select|set|shift|source|then|time|trap|true|type|ulimit|umask|unalias|unset|until|wait|while)\b/g,
  toml: /\b(true|false)\b/g,
  yaml: /\b(true|false|null|yes|no|on|off)\b/g,
  proto: /\b(syntax|package|import|option|message|enum|service|rpc|returns|repeated|optional|required|oneof|map|reserved|to|true|false)\b/g,
  docker: /\b(FROM|RUN|CMD|LABEL|MAINTAINER|EXPOSE|ENV|ADD|COPY|ENTRYPOINT|VOLUME|USER|WORKDIR|ARG|ONBUILD|STOPSIGNAL|HEALTHCHECK|SHELL)\b/g
};
KW.tsx = KW.ts; KW.jsx = KW.js; KW.json = null; KW.html = null; KW.css = null; KW.txt = null;
function highlight(src, lang) {
  const re = KW[lang];
  const parts = [];
  const n = src.length;
  let i = 0;
  function pushPlain(t) {
    if (!t) return;
    let e = esc(t);
    if (re) e = e.replace(re, '<span class="k">$1</span>');
    e = e.replace(/\b(0x[0-9a-fA-F]+|\d+\.?\d*)\b/g, '<span class="n">$1</span>');
    parts.push(e);
  }
  while (i < n) {
    const c = src[i];
    const c2 = src[i+1];
    if (lang === "py" && c === "#") {
      let j = src.indexOf("\n", i); if (j < 0) j = n;
      parts.push('<span class="c">' + esc(src.slice(i, j)) + "</span>");
      i = j; continue;
    }
    if ((lang === "js" || lang === "ts" || lang === "tsx" || lang === "jsx" || lang === "rs" || lang === "go" || lang === "c" || lang === "cpp" || lang === "java" || lang === "kt" || lang === "swift" || lang === "cs" || lang === "zig" || lang === "proto") && c === "/" && c2 === "/") {
      let j = src.indexOf("\n", i); if (j < 0) j = n;
      parts.push('<span class="c">' + esc(src.slice(i, j)) + "</span>");
      i = j; continue;
    }
    if ((lang === "js" || lang === "ts" || lang === "c" || lang === "cpp" || lang === "java" || lang === "rs" || lang === "go" || lang === "css" || lang === "kt" || lang === "swift" || lang === "cs") && c === "/" && c2 === "*") {
      let j = src.indexOf("*/", i+2); j = j < 0 ? n : j+2;
      parts.push('<span class="c">' + esc(src.slice(i, j)) + "</span>");
      i = j; continue;
    }
    if (c === '"' || c === "'" || (c === "`" && (lang === "js" || lang === "ts" || lang === "tsx" || lang === "jsx" || lang === "sh"))) {
      let j = i + 1;
      while (j < n) {
        if (src[j] === "\\") { j += 2; continue; }
        if (src[j] === c) { j++; break; }
        j++;
      }
      parts.push('<span class="s">' + esc(src.slice(i, j)) + "</span>");
      i = j; continue;
    }
    let j = i + 1;
    while (j < n && src[j] !== '"' && src[j] !== "'" && src[j] !== "`" && src[j] !== "#" && !(src[j] === "/" && (src[j+1] === "/" || src[j+1] === "*"))) j++;
    pushPlain(src.slice(i, j));
    i = j;
  }
  return parts.join("");
}
"###;

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
        assert!(html.contains("highlight("));
        assert!(html.contains(">rs<"), "language badge: {html}");
    }

    #[test]
    fn start_line_is_baked() {
        let html = document(Path::new("a.rs"), "a\nb\nc\n", Some(3), Some(1));
        assert!(html.contains("const START_LINE = 3;"), "{html}");
        assert!(html.contains("a.rs"));
    }

    #[test]
    fn markdown_renders_heading_and_code() {
        let html = document(
            Path::new("note.md"),
            "# Hello\n\n```rs\nfn main() {}\n```\n",
            None,
            None,
        );
        assert!(html.contains("<h1>Hello</h1>"), "{html}");
        assert!(html.contains("<article>"), "{html}");
        assert!(!html.contains("<textarea"), "{html}");
        assert!(
            html.contains("language-rs") || html.contains("fn main"),
            "{html}"
        );
    }

    #[test]
    fn language_from_extension() {
        assert_eq!(language_of(Path::new("src/main.rs")), "rs");
        assert_eq!(language_of(Path::new("app.ts")), "ts");
        assert_eq!(language_of(Path::new("Dockerfile")), "docker");
        assert_eq!(language_of(Path::new("Makefile")), "sh");
        assert_eq!(language_of(Path::new("a.unknown")), "txt");
        assert!(is_markdown(Path::new("README.MD")));
        assert!(!is_markdown(Path::new("main.rs")));
    }
}
