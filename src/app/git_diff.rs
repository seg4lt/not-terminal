use std::collections::BTreeMap;
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
    let file_tree = render_file_tree(snapshot);
    let sections = snapshot
        .sections
        .iter()
        .map(render_section)
        .collect::<Vec<_>>()
        .join("");
    let body = format!(
        "<div class=\"diff-shell\"><aside class=\"file-tree-panel\">{}</aside><main class=\"diff-main\"><div class=\"view-toolbar\"><button class=\"toolbar-btn\" type=\"button\" data-action=\"toggle-tree\" title=\"Show file tree\" aria-label=\"Show file tree\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"toggle-fullscreen\" title=\"Enter fullscreen\" aria-label=\"Enter fullscreen\">{}</button></div><div class=\"hero\"><div class=\"hero-label\">Diff</div><h1>{}</h1><p>{}</p></div>{}</main></div>",
        file_tree,
        tree_icon(),
        fullscreen_icon(),
        escape_html(&title),
        escape_html(&snapshot.worktree_path),
        sections,
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
            .map(|file| render_file(file, section.label, &file_dom_id(section.label, &file.path)))
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

fn render_file(file: &DiffFile, section_label: &str, file_id: &str) -> String {
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
    let open_attr = if section_label == "Unstaged" {
        " open"
    } else {
        ""
    };

    format!(
        "<details id=\"{}\" class=\"file-card\" data-language=\"{}\" data-file-id=\"{}\" data-stage=\"{}\" data-search=\"{} {}\"{}><summary><span class=\"file-main\"><span class=\"file-path\">{}</span></span><span class=\"file-stats\"><span class=\"added\">+{}</span><span class=\"removed\">-{}</span></span></summary><div class=\"file-body\">{}</div></details>",
        escape_html(file_id),
        language,
        escape_html(file_id),
        escape_html(section_label),
        escape_html(&file.path),
        escape_html(section_label),
        open_attr,
        escape_html(&file.path),
        file.added,
        file.removed,
        hunks
    )
}

fn render_file_tree(snapshot: &DiffSnapshot) -> String {
    let mut unstaged_root = TreeDirectory::default();
    let mut staged_root = TreeDirectory::default();
    let mut unstaged_count = 0usize;
    let mut staged_count = 0usize;

    for section in &snapshot.sections {
        for file in &section.files {
            if section.label == "Unstaged" {
                insert_tree_file(&mut unstaged_root, file, section.label);
                unstaged_count += 1;
            } else {
                insert_tree_file(&mut staged_root, file, section.label);
                staged_count += 1;
            }
        }
    }

    let total_count = unstaged_count + staged_count;
    let unstaged_html = render_tree_stage_section(
        "Working Copy",
        "Live edits",
        "tree-stage-heading-unstaged",
        &unstaged_root,
        unstaged_count,
    );
    let staged_html = render_tree_stage_section(
        "Index",
        "Ready to commit",
        "tree-stage-heading-staged",
        &staged_root,
        staged_count,
    );

    format!(
        "<div class=\"file-tree-shell\"><div class=\"file-tree-header\"><div class=\"file-tree-count\">{} Changes</div></div><label class=\"file-tree-filter\"><input type=\"search\" data-role=\"file-filter\" placeholder=\"Filter files...\" spellcheck=\"false\"></label>{}{}</div>",
        total_count, unstaged_html, staged_html,
    )
}

fn file_dom_id(section_label: &str, path: &str) -> String {
    format!("{}-{}", slugify(section_label), slugify(path))
}

fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut last_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

#[derive(Default)]
struct TreeDirectory {
    directories: BTreeMap<String, TreeDirectory>,
    files: Vec<TreeFileEntry>,
}

struct TreeFileEntry {
    label: String,
    path: String,
    file_id: String,
    section_label: &'static str,
}

fn insert_tree_file(root: &mut TreeDirectory, file: &DiffFile, section_label: &'static str) {
    let mut current = root;
    if let Some(parent) = Path::new(&file.path).parent() {
        for segment in parent
            .iter()
            .filter_map(|value| value.to_str())
            .filter(|value| !value.is_empty() && *value != ".")
        {
            current = current.directories.entry(segment.to_string()).or_default();
        }
    }

    current.files.push(TreeFileEntry {
        label: file_name(&file.path),
        path: file.path.clone(),
        file_id: file_dom_id(section_label, &file.path),
        section_label,
    });
}

fn render_tree_directory_contents(directory: &TreeDirectory, depth: usize) -> String {
    let mut rendered = directory
        .directories
        .iter()
        .map(|(name, child)| render_tree_directory(name, child, depth))
        .collect::<Vec<_>>();

    let mut files = directory.files.iter().collect::<Vec<_>>();
    files.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then(left.section_label.cmp(right.section_label))
    });
    rendered.extend(files.into_iter().map(|file| render_tree_file(file, depth)));
    rendered.join("")
}

fn render_tree_directory(name: &str, directory: &TreeDirectory, depth: usize) -> String {
    format!(
        "<details class=\"tree-group tree-dir\" open><summary class=\"tree-row tree-row-dir\" style=\"--depth:{}\"><span class=\"tree-caret\"></span><span class=\"tree-icon tree-icon-folder\">{}</span><span class=\"tree-label\">{}</span></summary><div class=\"tree-children\">{}</div></details>",
        depth,
        folder_icon(),
        escape_html(name),
        render_tree_directory_contents(directory, depth + 1)
    )
}

fn render_tree_file(file: &TreeFileEntry, depth: usize) -> String {
    let stage_class = if file.section_label == "Unstaged" {
        "tree-stage-unstaged"
    } else {
        "tree-stage-staged"
    };

    format!(
        "<button class=\"tree-row tree-file\" type=\"button\" style=\"--depth:{}\" data-file-target=\"{}\" data-filter-text=\"{} {}\"><span class=\"tree-row-spacer\"></span><span class=\"tree-icon tree-icon-file\">{}</span><span class=\"tree-label\">{}</span><span class=\"tree-stage-dot {}\"></span></button>",
        depth,
        escape_html(&file.file_id),
        escape_html(&file.path),
        escape_html(file.section_label),
        file_icon(),
        escape_html(&file.label),
        stage_class,
    )
}

fn render_tree_stage_section(
    title: &str,
    subtitle: &str,
    stage_class: &str,
    root: &TreeDirectory,
    count: usize,
) -> String {
    let body = if root.directories.is_empty() && root.files.is_empty() {
        String::from("<div class=\"tree-empty\">Nothing here</div>")
    } else {
        render_tree_directory_contents(root, 0)
    };

    format!(
        "<section class=\"file-tree-stage\"><div class=\"file-tree-stage-header\"><div class=\"file-tree-stage-copy\"><div class=\"file-tree-stage-title {}\">{}</div><div class=\"file-tree-stage-subtitle\">{}</div></div><div class=\"file-tree-stage-count\">{}</div></div><div class=\"file-tree-groups\">{}</div></section>",
        stage_class,
        escape_html(title),
        escape_html(subtitle),
        count,
        body
    )
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string())
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
.diff-shell {
  display: grid;
  grid-template-columns: 0 minmax(0, 1fr);
  gap: 18px;
  align-items: start;
}
body.tree-open .diff-shell {
  grid-template-columns: minmax(240px, 280px) minmax(0, 1fr);
}
.diff-main {
  min-width: 0;
}
.view-toolbar {
  position: sticky;
  top: 0;
  z-index: 30;
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-bottom: 8px;
  padding: 2px 0 10px;
  background: linear-gradient(180deg, rgba(15, 16, 17, 0.98) 0%, rgba(15, 16, 17, 0.92) 72%, rgba(15, 16, 17, 0) 100%);
}
.toolbar-btn {
  width: 42px;
  height: 42px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: rgba(33, 34, 37, 0.96);
  color: #d9dde3;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
}
.toolbar-btn:hover {
  background: rgba(48, 49, 54, 0.98);
}
.toolbar-btn.is-active {
  border-color: rgba(104, 156, 255, 0.36);
  background: rgba(39, 46, 60, 0.98);
  color: #9ec6ff;
}
.toolbar-btn svg {
  width: 18px;
  height: 18px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.9;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.file-tree-panel {
  position: sticky;
  top: 18px;
  max-height: calc(100vh - 36px);
  overflow: auto;
  opacity: 0;
  pointer-events: none;
  transform: translateX(-12px);
  transition: opacity 140ms ease, transform 140ms ease;
}
body.tree-open .file-tree-panel {
  opacity: 1;
  pointer-events: auto;
  transform: translateX(0);
}
.file-tree-shell {
  padding: 6px 0 0;
}
.file-tree-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
  padding: 0 2px;
}
.file-tree-count {
  font-size: 14px;
  font-weight: 700;
  color: #cfd4db;
}
.file-tree-filter {
  display: block;
  margin-bottom: 16px;
}
.file-tree-filter input {
  width: 100%;
  padding: 11px 14px;
  border: 1px solid rgba(255,255,255,0.08);
  border-radius: 12px;
  background: rgba(51, 52, 56, 0.92);
  color: var(--text);
  font: inherit;
  font-size: 15px;
  line-height: 1.2;
}
.file-tree-filter input::placeholder {
  color: #a0a5ad;
}
.file-tree-stage {
  margin-bottom: 18px;
}
.file-tree-stage-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  margin: 0 2px 10px;
}
.file-tree-stage-copy {
  min-width: 0;
}
.file-tree-stage-title {
  font-size: 12px;
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
.tree-stage-heading-unstaged {
  color: #7ee7a7;
}
.tree-stage-heading-staged {
  color: #9ec6ff;
}
.file-tree-stage-subtitle {
  margin-top: 2px;
  color: #8f96a0;
  font-size: 11px;
  font-weight: 600;
}
.file-tree-stage-count {
  flex: none;
  min-width: 24px;
  padding: 2px 8px;
  border-radius: 999px;
  background: rgba(255,255,255,0.06);
  color: #cbd1d9;
  font-size: 12px;
  font-weight: 700;
  text-align: center;
}
.file-tree-groups {
  display: block;
}
.tree-group {
  display: block;
}
.tree-group > summary::marker,
.tree-group > summary::-webkit-details-marker {
  display: none;
  content: "";
}
.tree-children {
  display: block;
}
.tree-row {
  --depth: 0;
  position: relative;
  width: 100%;
  min-height: 40px;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 12px 7px calc(10px + (var(--depth) * 26px));
  border: 0;
  background: transparent;
  color: var(--text);
  text-align: left;
}
.tree-row::before {
  content: "";
  position: absolute;
  left: calc(18px + (var(--depth) * 26px));
  top: 0;
  bottom: 0;
  width: 1px;
  background: rgba(255,255,255,0.08);
}
.tree-row-dir {
  cursor: pointer;
  color: #c7ccd4;
  font-weight: 700;
}
.tree-file {
  cursor: pointer;
}
.tree-file:hover,
.tree-file.is-active {
  background: rgba(255,255,255,0.05);
}
.tree-file.is-active {
  outline: 1px solid rgba(104, 156, 255, 0.3);
  outline-offset: -1px;
}
.tree-caret,
.tree-row-spacer {
  position: relative;
  z-index: 1;
  width: 12px;
  flex: none;
  color: #c8ccd3;
}
.tree-caret::before {
  content: "⌄";
}
.tree-group:not([open]) > summary .tree-caret::before {
  content: "›";
}
.tree-row-spacer::before {
  content: "";
}
.tree-icon {
  position: relative;
  z-index: 1;
  width: 18px;
  height: 18px;
  flex: none;
  color: #cfd4db;
}
.tree-icon svg {
  width: 18px;
  height: 18px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.tree-label {
  min-width: 0;
  flex: 1;
  font-size: 14px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tree-stage-dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 999px;
  box-shadow: 0 0 0 2px rgba(255,255,255,0.04);
}
.tree-stage-unstaged {
  background: #7ee7a7;
}
.tree-stage-staged {
  background: #9ec6ff;
}
.tree-empty {
  padding: 16px 12px;
  color: var(--muted);
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
.file-card.is-active {
  border-color: rgba(104, 156, 255, 0.38);
  box-shadow: 0 0 0 1px rgba(104, 156, 255, 0.18);
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
.file-card > summary,
.file-card > summary * {
  cursor: pointer;
}
.file-card > summary {
  user-select: none;
  -webkit-user-select: none;
}
.file-card > summary::-webkit-details-marker {
  display: none;
}
.file-main {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
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
.filter-hidden {
  display: none !important;
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

  const body = document.body;
  const treeButton = document.querySelector('[data-action="toggle-tree"]');
  const fullscreenButton = document.querySelector('[data-action="toggle-fullscreen"]');
  const filterInput = document.querySelector('[data-role="file-filter"]');
  const fileCards = Array.from(document.querySelectorAll('.file-card'));
  const treeFiles = Array.from(document.querySelectorAll('.tree-file'));
  const treeGroups = Array.from(document.querySelectorAll('.tree-group'));

  function postNativeAction(action) {
    try {
      const handler = window.webkit?.messageHandlers?.notTerminalDiff;
      if (!handler) return false;
      handler.postMessage(action);
      return true;
    } catch (_error) {
      return false;
    }
  }

  function visibleCards() {
    return fileCards.filter((card) => !card.classList.contains('filter-hidden'));
  }

  function activeCard() {
    const activeId = body.dataset.activeFile || "";
    return fileCards.find((card) => card.dataset.fileId === activeId) || null;
  }

  function firstPreferredCard() {
    return visibleCards().find((card) => card.dataset.stage === 'Unstaged')
      || visibleCards()[0]
      || fileCards.find((card) => card.dataset.stage === 'Unstaged')
      || fileCards[0]
      || null;
  }

  function setActiveFile(fileId, options = {}) {
    const card = fileCards.find((candidate) => candidate.dataset.fileId === fileId);
    if (!card) return;

    body.dataset.activeFile = fileId;
    fileCards.forEach((candidate) => {
      candidate.classList.toggle('is-active', candidate === card);
    });
    treeFiles.forEach((item) => {
      item.classList.toggle('is-active', item.dataset.fileTarget === fileId);
    });

    if (!card.open) {
      card.open = true;
    }

    if (options.scroll) {
      card.scrollIntoView({ block: 'start', behavior: 'auto' });
    }
  }

  function syncToolbar() {
    const treeOpen = body.classList.contains('tree-open');
    const splitZoomed = fullscreenButton?.classList.contains('is-active') || false;

    if (treeButton) {
      treeButton.classList.toggle('is-active', treeOpen);
      treeButton.title = treeOpen ? 'Hide file tree' : 'Show file tree';
      treeButton.setAttribute('aria-label', treeButton.title);
    }

    if (fullscreenButton) {
      fullscreenButton.title = splitZoomed ? 'Exit fullscreen' : 'Enter fullscreen';
      fullscreenButton.setAttribute('aria-label', fullscreenButton.title);
    }
  }

  function toggleTree() {
    body.classList.toggle('tree-open');
    syncToolbar();
  }

  function toggleFullscreen() {
    if (!activeCard()) {
      const preferred = firstPreferredCard();
      if (!preferred) return;
      setActiveFile(preferred.dataset.fileId, { scroll: false });
    }

    if (!fullscreenButton) return;

    fullscreenButton.classList.toggle('is-active');
    syncToolbar();
    postNativeAction('toggle-split-zoom');

    const card = activeCard();
    if (card) {
      card.scrollIntoView({ block: 'start', behavior: 'auto' });
    }
  }

  function applyFilter() {
    const term = (filterInput?.value || '').trim().toLowerCase();

    treeFiles.forEach((item) => {
      const matches = !term || (item.dataset.filterText || '').toLowerCase().includes(term);
      item.classList.toggle('filter-hidden', !matches);
    });

    treeGroups.forEach((group) => {
      const anyVisible = group.querySelector('.tree-file:not(.filter-hidden)');
      group.classList.toggle('filter-hidden', !anyVisible);
      if (anyVisible) {
        group.open = true;
      }
    });

    document.querySelectorAll('.file-tree-stage').forEach((section) => {
      const hasVisibleTreeRow = section.querySelector('.tree-file:not(.filter-hidden), .tree-dir:not(.filter-hidden)');
      section.classList.toggle('filter-hidden', !hasVisibleTreeRow && !!term);
    });

    fileCards.forEach((card) => {
      const haystack = ((card.dataset.search || '') + ' ' + (card.dataset.stage || '')).toLowerCase();
      const matches = !term || haystack.includes(term);
      card.classList.toggle('filter-hidden', !matches);
    });

    document.querySelectorAll('.diff-section').forEach((section) => {
      const hasVisibleCard = section.querySelector('.file-card:not(.filter-hidden)');
      const emptyState = section.querySelector('.empty-state');
      section.classList.toggle('filter-hidden', !hasVisibleCard && !emptyState);
    });

    const current = activeCard();
    if (!current || current.classList.contains('filter-hidden')) {
      const preferred = firstPreferredCard();
      if (preferred) {
        setActiveFile(preferred.dataset.fileId, { scroll: false });
      }
    }
  }

  treeButton?.addEventListener('click', toggleTree);
  fullscreenButton?.addEventListener('click', toggleFullscreen);
  filterInput?.addEventListener('input', applyFilter);

  fileCards.forEach((card) => {
    const summary = card.querySelector('summary');
    summary?.addEventListener('click', (event) => {
      event.preventDefault();
      const nextOpen = !card.open;
      setActiveFile(card.dataset.fileId || '', { scroll: false });
      card.open = nextOpen;
    });
  });

  treeFiles.forEach((item) => {
    item.addEventListener('click', () => {
      const fileId = item.dataset.fileTarget || '';
      setActiveFile(fileId, { scroll: true });
    });
  });

  const preferred = firstPreferredCard();
  if (preferred) {
    setActiveFile(preferred.dataset.fileId || '', { scroll: false });
  }
  syncToolbar();
})();
"###
}

fn tree_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4.5" y="4.5" width="15" height="15" rx="2.5"></rect><path d="M12 8v8"></path><path d="M8 12h8"></path></svg>"#
}

fn fullscreen_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 4.5H4.5V8"></path><path d="M16 4.5h3.5V8"></path><path d="M8 19.5H4.5V16"></path><path d="M16 19.5h3.5V16"></path></svg>"#
}

fn folder_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 7.5A1.5 1.5 0 0 1 5 6h4l1.6 1.8H19A1.5 1.5 0 0 1 20.5 9.3v7.2A1.5 1.5 0 0 1 19 18H5a1.5 1.5 0 0 1-1.5-1.5Z"></path></svg>"#
}

fn file_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 4.5h6l4 4V19a1.5 1.5 0 0 1-1.5 1.5h-8A1.5 1.5 0 0 1 7 19V6A1.5 1.5 0 0 1 8.5 4.5Z"></path><path d="M14 4.5V9h4"></path></svg>"#
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
