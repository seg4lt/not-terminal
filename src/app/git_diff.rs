use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub(crate) struct DiffSnapshot {
    pub(crate) worktree_path: String,
    pub(crate) sections: Vec<DiffSection>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffSection {
    pub(crate) label: &'static str,
    pub(crate) empty_label: &'static str,
    pub(crate) files: Vec<DiffFile>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffFile {
    pub(crate) path: String,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffHunk {
    pub(crate) header: String,
    pub(crate) lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffLineKind {
    Context,
    Added,
    Removed,
    Note,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffLine {
    pub(crate) kind: DiffLineKind,
    pub(crate) old_line: Option<usize>,
    pub(crate) new_line: Option<usize>,
    pub(crate) text: String,
}

pub(crate) fn load_snapshot(worktree_path: &str) -> Result<DiffSnapshot, String> {
    let unstaged = load_section(
        worktree_path,
        "Unstaged",
        "No unstaged changes",
        &["diff", "--no-ext-diff"],
    )?;
    let staged = load_section(
        worktree_path,
        "Staged",
        "No staged changes",
        &["diff", "--no-ext-diff", "--staged"],
    )?;

    Ok(DiffSnapshot {
        worktree_path: worktree_path.to_string(),
        sections: vec![unstaged, staged],
    })
}

pub(crate) fn render_loading_html(worktree_path: &str) -> String {
    let title = repo_title(worktree_path);
    render_document(
        &title,
        &format!(
            "<div class=\"hero\"><div class=\"hero-label\">Diff</div><h1>{}</h1><p>Loading working tree changes…</p></div>",
            escape_html(&title)
        ),
    )
}

pub(crate) fn render_error_html(worktree_path: &str, error: &str) -> String {
    let title = repo_title(worktree_path);
    render_document(
        &title,
        &format!(
            "<div class=\"hero\"><div class=\"hero-label\">Diff</div><h1>{}</h1><p class=\"error\">{}</p></div>",
            escape_html(&title),
            escape_html(error)
        ),
    )
}

pub(crate) fn render_snapshot_html(snapshot: &DiffSnapshot) -> String {
    let title = repo_title(&snapshot.worktree_path);
    let sections = snapshot
        .sections
        .iter()
        .map(render_section)
        .collect::<Vec<_>>()
        .join("");
    let body = format!(
        "<div class=\"hero\"><div class=\"hero-label\">Diff</div><h1>{}</h1><p>{}</p></div>{}",
        escape_html(&title),
        escape_html(&snapshot.worktree_path),
        sections
    );
    render_document(&title, &body)
}

fn load_section(
    worktree_path: &str,
    label: &'static str,
    empty_label: &'static str,
    args: &[&str],
) -> Result<DiffSection, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("git {} failed with {}", args.join(" "), output.status)
        } else {
            stderr
        };
        return Err(message);
    }

    let patch = String::from_utf8(output.stdout)
        .map_err(|error| format!("git {} produced invalid UTF-8: {error}", args.join(" ")))?;

    Ok(DiffSection {
        label,
        empty_label,
        files: parse_patch(&patch),
    })
}

fn parse_patch(patch: &str) -> Vec<DiffFile> {
    let mut files = Vec::new();
    let mut current_file: Option<DiffFile> = None;
    let mut current_hunk: Option<usize> = None;
    let mut old_line = 0usize;
    let mut new_line = 0usize;

    for raw_line in patch.lines() {
        if let Some(path) = raw_line
            .strip_prefix("diff --git ")
            .and_then(parse_diff_git_path)
        {
            if let Some(file) = current_file.take() {
                files.push(file);
            }

            current_file = Some(DiffFile {
                path,
                added: 0,
                removed: 0,
                hunks: Vec::new(),
            });
            current_hunk = None;
            continue;
        }

        let Some(file) = current_file.as_mut() else {
            continue;
        };

        if let Some(path) = raw_line.strip_prefix("+++ ") {
            if let Some(path) = normalize_patch_path(path) {
                file.path = path;
            }
            continue;
        }

        if let Some((next_old_line, next_new_line)) = parse_hunk_header(raw_line) {
            old_line = next_old_line;
            new_line = next_new_line;
            file.hunks.push(DiffHunk {
                header: raw_line.to_string(),
                lines: Vec::new(),
            });
            current_hunk = Some(file.hunks.len() - 1);
            continue;
        }

        let Some(hunk_idx) = current_hunk else {
            continue;
        };
        let hunk = &mut file.hunks[hunk_idx];

        if raw_line.starts_with('+') && !raw_line.starts_with("+++") {
            file.added += 1;
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Added,
                old_line: None,
                new_line: Some(new_line),
                text: raw_line[1..].to_string(),
            });
            new_line += 1;
        } else if raw_line.starts_with('-') && !raw_line.starts_with("---") {
            file.removed += 1;
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Removed,
                old_line: Some(old_line),
                new_line: None,
                text: raw_line[1..].to_string(),
            });
            old_line += 1;
        } else if let Some(context) = raw_line.strip_prefix(' ') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Context,
                old_line: Some(old_line),
                new_line: Some(new_line),
                text: context.to_string(),
            });
            old_line += 1;
            new_line += 1;
        } else if let Some(note) = raw_line.strip_prefix('\\') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Note,
                old_line: None,
                new_line: None,
                text: note.trim().to_string(),
            });
        }
    }

    if let Some(file) = current_file.take() {
        files.push(file);
    }

    files
}

fn parse_diff_git_path(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    let _a_path = parts.next()?;
    let b_path = parts.next()?;
    normalize_patch_path(b_path)
}

fn normalize_patch_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed == "/dev/null" {
        None
    } else if let Some(path) = trimmed.strip_prefix("a/") {
        Some(path.to_string())
    } else if let Some(path) = trimmed.strip_prefix("b/") {
        Some(path.to_string())
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    if !line.starts_with("@@ ") {
        return None;
    }

    let mut parts = line.split_whitespace();
    let _marker = parts.next()?;
    let old_spec = parts.next()?;
    let new_spec = parts.next()?;
    Some((parse_hunk_range(old_spec)?, parse_hunk_range(new_spec)?))
}

fn parse_hunk_range(spec: &str) -> Option<usize> {
    let without_sign = spec.strip_prefix(['-', '+'])?;
    let number = without_sign.split(',').next()?;
    number.parse().ok()
}

fn render_section(section: &DiffSection) -> String {
    let count = section.files.len();
    let files = if section.files.is_empty() {
        format!(
            "<div class=\"empty-state\">{}</div>",
            escape_html(section.empty_label)
        )
    } else {
        section
            .files
            .iter()
            .map(render_file)
            .collect::<Vec<_>>()
            .join("")
    };

    format!(
        "<section class=\"diff-section\"><div class=\"section-header\"><div class=\"section-title\">{}</div><div class=\"section-count\">{}</div></div>{}</section>",
        escape_html(section.label),
        count,
        files
    )
}

fn render_file(file: &DiffFile) -> String {
    let language = infer_language(&file.path);
    let hunks = if file.hunks.is_empty() {
        "<div class=\"file-empty\">No textual changes to display.</div>".to_string()
    } else {
        file.hunks
            .iter()
            .map(render_hunk)
            .collect::<Vec<_>>()
            .join("")
    };

    format!(
        "<details class=\"file-card\" data-language=\"{}\" open><summary><span class=\"file-path\">{}</span><span class=\"file-stats\"><span class=\"added\">+{}</span><span class=\"removed\">-{}</span></span></summary><div class=\"file-body\">{}</div></details>",
        language,
        escape_html(&file.path),
        file.added,
        file.removed,
        hunks
    )
}

fn render_hunk(hunk: &DiffHunk) -> String {
    let rows = render_hunk_rows(&hunk.lines);
    format!(
        "<div class=\"hunk\"><div class=\"hunk-header\">{}</div><div class=\"diff-grid\">{}</div></div>",
        escape_html(&hunk.header),
        rows
    )
}

fn render_hunk_rows(lines: &[DiffLine]) -> String {
    let mut rendered = String::new();
    let mut index = 0usize;

    while index < lines.len() {
        if lines[index].kind != DiffLineKind::Context {
            rendered.push_str(&render_row(&lines[index]));
            index += 1;
            continue;
        }

        let start = index;
        while index < lines.len() && lines[index].kind == DiffLineKind::Context {
            index += 1;
        }
        let context_run = &lines[start..index];
        const CONTEXT_LIMIT: usize = 8;
        const CONTEXT_VISIBLE: usize = 2;

        if context_run.len() > CONTEXT_LIMIT {
            for line in &context_run[..CONTEXT_VISIBLE] {
                rendered.push_str(&render_row(line));
            }
            let hidden_lines = &context_run[CONTEXT_VISIBLE..context_run.len() - CONTEXT_VISIBLE];
            rendered.push_str(&format!(
                "<details class=\"context-group\"><summary class=\"row row-gap\"><div class=\"line gap-line\"><span class=\"gap-caret\"></span></div><div class=\"code\"><span class=\"gap-label\">{} unmodified lines</span><span class=\"gap-action\"></span></div></summary><div class=\"context-hidden\">{}</div></details>",
                hidden_lines.len(),
                hidden_lines
                    .iter()
                    .map(render_row)
                    .collect::<Vec<_>>()
                    .join("")
            ));
            for line in &context_run[context_run.len() - CONTEXT_VISIBLE..] {
                rendered.push_str(&render_row(line));
            }
        } else {
            for line in context_run {
                rendered.push_str(&render_row(line));
            }
        }
    }

    rendered
}

fn render_row(line: &DiffLine) -> String {
    let row_class = match line.kind {
        DiffLineKind::Context => "row-context",
        DiffLineKind::Added => "row-added",
        DiffLineKind::Removed => "row-removed",
        DiffLineKind::Note => "row-note",
    };

    let display_line = line
        .new_line
        .or(line.old_line)
        .map(|value| value.to_string())
        .unwrap_or_default();
    if line.kind == DiffLineKind::Note {
        return format!(
            "<div class=\"row {}\"><div class=\"line\">{}</div><div class=\"code\">{}</div></div>",
            row_class,
            escape_html(&display_line),
            escape_html(&format!(r#"\ {}"#, line.text))
        );
    }

    format!(
        "<div class=\"row {}\"><div class=\"line\">{}</div><div class=\"code\" data-highlight=\"1\"><span class=\"code-content\">{}</span></div></div>",
        row_class,
        escape_html(&display_line),
        escape_html(&line.text)
    )
}

fn repo_title(worktree_path: &str) -> String {
    Path::new(worktree_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| String::from("Diff"))
}

fn render_document(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><style>{}</style></head><body>{}<script>{}</script></body></html>",
        escape_html(title),
        document_css(),
        body,
        document_js()
    )
}

fn document_css() -> &'static str {
    r#"
:root {
  color-scheme: dark;
  --bg: #111213;
  --panel: #18191b;
  --panel-2: #1f2023;
  --border: #2c2d31;
  --muted: #8e949d;
  --text: #eef2f6;
  --green-bg: rgba(76, 175, 80, 0.16);
  --green-edge: rgba(71, 214, 114, 0.9);
  --red-bg: rgba(244, 67, 54, 0.16);
  --red-edge: rgba(255, 96, 96, 0.9);
  --ctx-bg: #17181a;
  --gap-bg: #34363a;
  --hunk-bg: #141518;
}
* { box-sizing: border-box; }
html, body {
  margin: 0;
  background: linear-gradient(180deg, #121315 0%, #0f1011 100%);
  color: var(--text);
  font-family: ui-monospace, "SF Mono", Menlo, Monaco, monospace;
}
body {
  min-height: 100vh;
  padding: 18px;
}
.hero {
  margin-bottom: 18px;
  padding: 10px 6px 2px;
}
.hero-label {
  color: var(--muted);
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.hero h1 {
  margin: 6px 0 8px;
  font-size: 26px;
  line-height: 1.15;
}
.hero p {
  margin: 0;
  color: var(--muted);
  word-break: break-all;
}
.hero .error {
  color: #ff9b9b;
}
.diff-section {
  margin-bottom: 18px;
}
.section-header {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
  padding: 8px 12px;
  background: rgba(33, 34, 37, 0.96);
  border: 1px solid var(--border);
  border-radius: 999px;
}
.section-title {
  font-size: 16px;
  font-weight: 700;
}
.section-count {
  min-width: 28px;
  padding: 2px 8px;
  border-radius: 999px;
  text-align: center;
  background: rgba(255,255,255,0.08);
  color: #d9dde3;
  font-size: 13px;
}
.empty-state, .file-empty {
  padding: 18px;
  border: 1px solid var(--border);
  border-radius: 16px;
  background: rgba(24, 25, 27, 0.88);
  color: var(--muted);
}
.file-card {
  margin-bottom: 14px;
  border: 1px solid var(--border);
  border-radius: 16px;
  background: rgba(24, 25, 27, 0.96);
  overflow: hidden;
}
.file-card > summary {
  list-style: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 20px;
  cursor: pointer;
  background: rgba(30, 31, 34, 0.95);
}
.file-card > summary::-webkit-details-marker {
  display: none;
}
.file-path {
  font-size: 15px;
  font-weight: 700;
  word-break: break-word;
}
.file-stats {
  display: inline-flex;
  gap: 10px;
  font-size: 14px;
  white-space: nowrap;
}
.added { color: #42d17f; }
.removed { color: #ff5f61; }
.file-body {
  padding: 0 10px 10px;
}
.hunk {
  margin-top: 10px;
  border: 1px solid rgba(55, 57, 62, 0.85);
  border-radius: 12px;
  overflow: hidden;
  background: var(--hunk-bg);
}
.hunk-header {
  padding: 8px 14px;
  background: rgba(29, 30, 34, 0.98);
  color: var(--muted);
  font-size: 12px;
}
.diff-grid {
  display: block;
}
.row {
  display: grid;
  grid-template-columns: 96px minmax(0, 1fr);
  align-items: stretch;
  min-height: 24px;
}
.row .line,
.row .code {
  padding: 3px 10px;
}
.row .line {
  color: #7f8791;
  text-align: right;
  border-right: 1px solid rgba(55, 57, 62, 0.65);
  user-select: none;
  -webkit-user-select: none;
}
.row .line,
.row .line * {
  -webkit-touch-callout: none;
}
.row .line::selection,
.row .line *::selection {
  background: transparent;
}
.row-added .line {
  color: #42d17f;
  background: rgba(71, 214, 114, 0.12);
  border-left: 4px solid var(--green-edge);
}
.row-removed .line {
  color: #ff5f61;
  background: rgba(255, 96, 96, 0.12);
  border-left: 4px solid var(--red-edge);
}
.row .code {
  white-space: pre-wrap;
  word-break: break-word;
}
.tok-comment { color: #7d8590; font-style: italic; }
.tok-string { color: #f6c177; }
.tok-number { color: #8bd5ff; }
.tok-keyword { color: #b388ff; font-weight: 600; }
.tok-type { color: #7ee7d8; }
.tok-fn { color: #ffad66; }
.tok-const { color: #ff7aa2; }
.tok-macro { color: #9bd0ff; }
.tok-attr { color: #8bcf7b; }
.tok-operator { color: #d1d7e0; }
.tok-punct { color: #aab2bf; }
.tok-plain { color: inherit; }
.row-context {
  background: var(--ctx-bg);
}
.row-added {
  background: linear-gradient(90deg, rgba(71, 214, 114, 0.16) 0%, var(--green-bg) 100%);
}
.row-removed {
  background: linear-gradient(90deg, rgba(255, 96, 96, 0.18) 0%, var(--red-bg) 100%);
}
.row-note {
  background: rgba(34, 35, 39, 0.94);
  color: var(--muted);
  font-style: italic;
}
.row-gap {
  background: var(--gap-bg);
  color: #c0c5cc;
}
.context-group {
  display: block;
}
.context-group > summary {
  list-style: none;
  cursor: pointer;
}
.context-group > summary::marker {
  content: "";
}
.context-group > summary::-webkit-details-marker {
  display: none;
}
.gap-line {
  display: flex;
  align-items: center;
  justify-content: center;
}
.gap-caret::before {
  content: "▾";
  color: #b7bcc6;
  font-size: 14px;
  line-height: 1;
}
.context-group[open] .gap-caret::before {
  content: "▴";
}
.row-gap .code {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 6px 12px;
  border-radius: 10px;
  margin: 8px;
  background: rgba(255,255,255,0.06);
  transition: background 120ms ease, color 120ms ease;
}
.row-gap:hover .code {
  background: rgba(255,255,255,0.1);
}
.gap-label {
  min-width: 0;
}
.gap-action {
  flex: none;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: #9ec6ff;
}
.context-group[open] .gap-action {
  color: #aeb7c4;
}
.context-group[open] .gap-action::before {
  content: "Collapse";
}
.context-group:not([open]) .gap-action::before {
  content: "Expand";
}
.context-hidden {
  display: block;
}
"#
}

fn document_js() -> &'static str {
    r###"
(function () {
  const KEYWORDS = {
    rust: new Set(["as","async","await","break","const","continue","crate","dyn","else","enum","extern","false","fn","for","if","impl","in","let","loop","match","mod","move","mut","pub","ref","return","self","Self","static","struct","super","trait","true","type","unsafe","use","where","while"]),
    js: new Set(["async","await","break","case","catch","class","const","continue","default","else","export","extends","false","finally","for","from","function","if","import","in","let","new","null","return","static","super","switch","this","throw","true","try","typeof","undefined","var","while","yield"]),
    json: new Set(["true","false","null"]),
    toml: new Set(["true","false"])
  };

  const TYPES = {
    rust: /^[A-Z][A-Za-z0-9_]*$/,
    js: /^(Promise|Array|Object|Map|Set|Date|Error|RegExp|String|Number|Boolean)$/
  };

  const FN_CALL = /^[A-Za-z_][A-Za-z0-9_]*$/;
  const OPERATORS = new Set(["=", ">", "<", "!", "+", "-", "*", "/", "%", "&", "|", "^", "~", "?", ":"]);
  const PUNCT = new Set(["(", ")", "[", "]", "{", "}", ".", ",", ";"]);

  function escapeHtml(value) {
    return value
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function languageFor(element) {
    return element.closest(".file-card")?.dataset.language || "plain";
  }

  function keywordSet(language) {
    if (language === "ts" || language === "tsx" || language === "jsx") return KEYWORDS.js;
    return KEYWORDS[language] || new Set();
  }

  function typePattern(language) {
    if (language === "ts" || language === "tsx" || language === "jsx") return TYPES.js;
    return TYPES[language];
  }

  function token(className, value) {
    if (!value) return "";
    return `<span class="${className}">${escapeHtml(value)}</span>`;
  }

  function isIdentifierStart(ch) {
    return /[A-Za-z_]/.test(ch);
  }

  function isIdentifierPart(ch) {
    return /[A-Za-z0-9_]/.test(ch);
  }

  function readWhile(input, index, predicate) {
    let end = index;
    while (end < input.length && predicate(input[end])) end += 1;
    return input.slice(index, end);
  }

  function renderIdentifier(input, start, language) {
    const value = readWhile(input, start, isIdentifierPart);
    const next = input[start + value.length];
    const previous = start > 0 ? input[start - 1] : "";
    const keywords = keywordSet(language);
    const typeMatcher = typePattern(language);

    if (previous === "#") return [token("tok-attr", value), value.length];
    if (next === "!") return [token("tok-macro", value) + token("tok-operator", "!"), value.length + 1];
    if (keywords.has(value)) return [token("tok-keyword", value), value.length];
    if (typeMatcher && typeMatcher.test(value)) return [token("tok-type", value), value.length];
    if (/^[A-Z0-9_]+$/.test(value) && value.length > 1) return [token("tok-const", value), value.length];

    const rest = input.slice(start + value.length);
    const fnMatch = rest.match(/^(\s*)(\()/);
    if (FN_CALL.test(value) && fnMatch) {
      return [token("tok-fn", value), value.length];
    }

    return [token("tok-plain", value), value.length];
  }

  function highlightLine(input, language) {
    let index = 0;
    let out = "";

    while (index < input.length) {
      const ch = input[index];
      const next = input[index + 1] || "";

      if ((language === "rust" || language === "js" || language === "ts" || language === "tsx" || language === "jsx") && ch === "/" && next === "/") {
        out += token("tok-comment", input.slice(index));
        break;
      }

      if ((language === "py" || language === "toml" || language === "yaml" || language === "yml" || language === "sh" || language === "bash") && ch === "#") {
        out += token("tok-comment", input.slice(index));
        break;
      }

      if (ch === "\"" || ch === "'" || ch === "`") {
        let end = index + 1;
        let escaped = false;
        while (end < input.length) {
          const current = input[end];
          if (!escaped && current === ch) {
            end += 1;
            break;
          }
          escaped = !escaped && current === "\\";
          if (!escaped && current !== "\\") escaped = false;
          end += 1;
        }
        out += token("tok-string", input.slice(index, end));
        index = end;
        continue;
      }

      if (/[0-9]/.test(ch)) {
        const value = readWhile(input, index, (c) => /[0-9A-Fa-f_xob\.]/.test(c));
        out += token("tok-number", value);
        index += value.length;
        continue;
      }

      if (isIdentifierStart(ch)) {
        const [rendered, consumed] = renderIdentifier(input, index, language);
        out += rendered;
        index += consumed;
        continue;
      }

      if (OPERATORS.has(ch)) {
        out += token("tok-operator", ch);
        index += 1;
        continue;
      }

      if (PUNCT.has(ch)) {
        out += token("tok-punct", ch);
        index += 1;
        continue;
      }

      out += escapeHtml(ch);
      index += 1;
    }

    return out;
  }

  document.querySelectorAll('.code[data-highlight="1"] .code-content').forEach((node) => {
    const language = languageFor(node);
    const source = node.textContent || "";
    node.innerHTML = highlightLine(source, language);
  });
})();
"###
}

fn infer_language(path: &str) -> &'static str {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "rs" => "rust",
        "js" | "mjs" | "cjs" => "js",
        "ts" => "ts",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "json" => "json",
        "toml" => "toml",
        "py" => "py",
        "go" => "go",
        "java" => "java",
        "kt" => "kotlin",
        "swift" => "swift",
        "c" | "h" | "m" | "mm" | "cc" | "cpp" | "hpp" => "cpp",
        "zig" => "zig",
        "sh" | "zsh" | "bash" => "sh",
        "yml" | "yaml" => "yaml",
        "md" => "md",
        _ => "plain",
    }
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
