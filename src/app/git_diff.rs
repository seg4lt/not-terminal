use std::collections::BTreeMap;
use std::fs;
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
    pub(crate) lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FileStage {
    Working,
    Index,
    Both,
}

#[derive(Debug, Clone)]
struct MergedDiffFile {
    path: String,
    added: usize,
    removed: usize,
    hunks: Vec<MergedDiffHunk>,
    stage: FileStage,
    source_lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct MergedDiffHunk {
    lines: Vec<DiffLine>,
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
    pub(crate) inline_ranges: Vec<(usize, usize)>,
}

pub(crate) fn load_snapshot(worktree_path: &str) -> Result<DiffSnapshot, String> {
    let unstaged = load_section(worktree_path, "Unstaged", &["diff", "--no-ext-diff"])?;
    let staged = load_section(
        worktree_path,
        "Staged",
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
    let files = merged_files(snapshot);
    let total_added = files.iter().map(|file| file.added).sum::<usize>();
    let total_removed = files.iter().map(|file| file.removed).sum::<usize>();
    let file_tree = render_file_tree(&files);
    let sections = render_snapshot_files(&files);
    let body = format!(
        "<div class=\"diff-shell\"><div class=\"view-toolbar\"><div class=\"view-toolbar-meta\"><span class=\"toolbar-total toolbar-total-added\">+{}</span><span class=\"toolbar-total toolbar-total-removed\">-{}</span></div><div class=\"view-toolbar-search\"><label class=\"toolbar-search\"><span class=\"toolbar-search-icon\">{}</span><input type=\"search\" data-role=\"content-search\" placeholder=\"Search diff...\" spellcheck=\"false\"></label><button class=\"toolbar-btn\" type=\"button\" data-action=\"prev-match\" title=\"Previous match\" aria-label=\"Previous match\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"next-match\" title=\"Next match\" aria-label=\"Next match\">{}</button><span class=\"toolbar-search-count\" data-role=\"search-count\">0</span></div><div class=\"view-toolbar-actions\"><button class=\"toolbar-btn\" type=\"button\" data-action=\"toggle-tree\" title=\"Show file tree\" aria-label=\"Show file tree\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"prev-file\" title=\"Previous file\" aria-label=\"Previous file\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"next-file\" title=\"Next file\" aria-label=\"Next file\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"prev-hunk\" title=\"Previous change\" aria-label=\"Previous change\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"next-hunk\" title=\"Next change\" aria-label=\"Next change\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"toggle-collapse\" title=\"Collapse files\" aria-label=\"Collapse files\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"toggle-fullscreen\" title=\"Enter fullscreen\" aria-label=\"Enter fullscreen\">{}</button><button class=\"toolbar-btn\" type=\"button\" data-action=\"close-diff\" title=\"Close diff\" aria-label=\"Close diff\">{}</button></div></div><div class=\"diff-zoom-surface\"><aside class=\"file-tree-panel\">{}</aside><main class=\"diff-main\"><div class=\"hero\"><div class=\"hero-label\">Diff</div><h1>{}</h1><p>{}</p></div>{}</main></div></div>",
        total_added,
        total_removed,
        search_icon(),
        chevron_up_icon(),
        chevron_down_icon(),
        tree_icon(),
        chevron_up_icon(),
        chevron_down_icon(),
        jump_up_icon(),
        jump_down_icon(),
        collapse_icon(),
        fullscreen_icon(),
        close_icon(),
        file_tree,
        escape_html(&title),
        escape_html(&snapshot.worktree_path),
        sections,
    );
    render_document(&title, &body)
}

pub(crate) fn inject_preserved_state(html: &str, state_json: &str) -> String {
    let escaped_state = state_json.replace("</", "<\\/");
    let bootstrap = format!(
        "<script>window.__NOT_TERMINAL_DIFF_INITIAL_STATE__ = {};</script><script>",
        escaped_state
    );
    html.replacen("<script>", &bootstrap, 1)
}

fn render_snapshot_files(files: &[MergedDiffFile]) -> String {
    let total_count = files.len();

    if total_count == 0 {
        return String::from("<div class=\"empty-state\">No changes to display.</div>");
    }

    let rendered = files.iter().map(render_file).collect::<Vec<_>>().join("");

    format!(
        "<section class=\"diff-section\"><div class=\"section-header\"><div class=\"section-title\">Changes</div><div class=\"section-count\">{}</div></div>{}</section>",
        total_count, rendered
    )
}

fn load_section(
    worktree_path: &str,
    label: &'static str,
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
            file.hunks.push(DiffHunk { lines: Vec::new() });
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
                inline_ranges: Vec::new(),
            });
            new_line += 1;
        } else if raw_line.starts_with('-') && !raw_line.starts_with("---") {
            file.removed += 1;
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Removed,
                old_line: Some(old_line),
                new_line: None,
                text: raw_line[1..].to_string(),
                inline_ranges: Vec::new(),
            });
            old_line += 1;
        } else if let Some(context) = raw_line.strip_prefix(' ') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Context,
                old_line: Some(old_line),
                new_line: Some(new_line),
                text: context.to_string(),
                inline_ranges: Vec::new(),
            });
            old_line += 1;
            new_line += 1;
        } else if let Some(note) = raw_line.strip_prefix('\\') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Note,
                old_line: None,
                new_line: None,
                text: note.trim().to_string(),
                inline_ranges: Vec::new(),
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

impl FileStage {
    fn from_section_label(label: &str) -> Self {
        match label {
            "Staged" => Self::Index,
            _ => Self::Working,
        }
    }

    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Both, _) | (_, Self::Both) => Self::Both,
            (Self::Working, Self::Working) => Self::Working,
            (Self::Index, Self::Index) => Self::Index,
            _ => Self::Both,
        }
    }

    fn dom_label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Index => "index",
            Self::Both => "both",
        }
    }

    fn meta_label(self) -> &'static str {
        match self {
            Self::Working => "working copy",
            Self::Index => "index",
            Self::Both => "working + index",
        }
    }

    fn tree_class(self) -> &'static str {
        match self {
            Self::Working => "tree-stage-working",
            Self::Index => "tree-stage-index",
            Self::Both => "tree-stage-both",
        }
    }
}

fn merged_files(snapshot: &DiffSnapshot) -> Vec<MergedDiffFile> {
    let mut merged: Vec<MergedDiffFile> = Vec::new();

    for section in &snapshot.sections {
        let stage = FileStage::from_section_label(section.label);
        for file in &section.files {
            if let Some(existing) = merged
                .iter_mut()
                .find(|candidate| candidate.path == file.path)
            {
                existing.added += file.added;
                existing.removed += file.removed;
                existing.stage = existing.stage.merge(stage);
                existing
                    .hunks
                    .extend(file.hunks.iter().cloned().map(|hunk| {
                        let mut lines = hunk.lines;
                        annotate_inline_changes(&mut lines);
                        MergedDiffHunk { lines }
                    }));
            } else {
                let mut hunks = Vec::with_capacity(file.hunks.len());
                for hunk in &file.hunks {
                    let mut lines = hunk.lines.clone();
                    annotate_inline_changes(&mut lines);
                    hunks.push(MergedDiffHunk { lines });
                }

                merged.push(MergedDiffFile {
                    path: file.path.clone(),
                    added: file.added,
                    removed: file.removed,
                    hunks,
                    stage,
                    source_lines: load_source_lines(&snapshot.worktree_path, &file.path),
                });
            }
        }
    }

    for file in &mut merged {
        file.hunks.sort_by_key(hunk_display_start);
    }

    merged
}

fn load_source_lines(worktree_path: &str, file_path: &str) -> Vec<String> {
    let full_path = Path::new(worktree_path).join(file_path);
    let Ok(source) = fs::read_to_string(full_path) else {
        return Vec::new();
    };

    source.lines().map(ToOwned::to_owned).collect()
}

fn annotate_inline_changes(lines: &mut [DiffLine]) {
    let mut index = 0usize;

    while index < lines.len() {
        if lines[index].kind != DiffLineKind::Removed {
            index += 1;
            continue;
        }

        let removed_start = index;
        while index < lines.len() && lines[index].kind == DiffLineKind::Removed {
            index += 1;
        }
        let removed_len = index - removed_start;

        let added_start = index;
        while index < lines.len() && lines[index].kind == DiffLineKind::Added {
            index += 1;
        }

        let added_len = index - added_start;
        if removed_len == 0 || added_len == 0 {
            continue;
        }

        let block_end = added_start + added_len;
        let (removed_block, added_block) =
            lines[removed_start..block_end].split_at_mut(removed_len);
        for pair_index in 0..removed_len.min(added_len) {
            let (removed_ranges, added_ranges) = compute_inline_ranges(
                &removed_block[pair_index].text,
                &added_block[pair_index].text,
            );
            removed_block[pair_index].inline_ranges = removed_ranges;
            added_block[pair_index].inline_ranges = added_ranges;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineKind {
    Word,
    Space,
    Punct,
}

#[derive(Clone, Copy)]
struct InlineToken<'a> {
    start: usize,
    text: &'a str,
}

fn compute_inline_ranges(left: &str, right: &str) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let left_tokens = tokenize_inline(left);
    let right_tokens = tokenize_inline(right);

    if left_tokens.is_empty() || right_tokens.is_empty() {
        return fallback_inline_ranges(left, right);
    }

    let mut lcs = vec![vec![0usize; right_tokens.len() + 1]; left_tokens.len() + 1];
    for left_index in (0..left_tokens.len()).rev() {
        for right_index in (0..right_tokens.len()).rev() {
            lcs[left_index][right_index] =
                if left_tokens[left_index].text == right_tokens[right_index].text {
                    lcs[left_index + 1][right_index + 1] + 1
                } else {
                    lcs[left_index + 1][right_index].max(lcs[left_index][right_index + 1])
                };
        }
    }

    let mut shared_left = vec![false; left_tokens.len()];
    let mut shared_right = vec![false; right_tokens.len()];
    let mut left_index = 0usize;
    let mut right_index = 0usize;
    while left_index < left_tokens.len() && right_index < right_tokens.len() {
        if left_tokens[left_index].text == right_tokens[right_index].text {
            shared_left[left_index] = true;
            shared_right[right_index] = true;
            left_index += 1;
            right_index += 1;
        } else if lcs[left_index + 1][right_index] >= lcs[left_index][right_index + 1] {
            left_index += 1;
        } else {
            right_index += 1;
        }
    }

    let left_ranges = collect_inline_ranges(left, &left_tokens, &shared_left);
    let right_ranges = collect_inline_ranges(right, &right_tokens, &shared_right);
    if left_ranges.is_empty() && right_ranges.is_empty() && left != right {
        fallback_inline_ranges(left, right)
    } else {
        (left_ranges, right_ranges)
    }
}

fn tokenize_inline(input: &str) -> Vec<InlineToken<'_>> {
    let mut tokens = Vec::new();
    let mut current_start = None;
    let mut current_kind = InlineKind::Word;

    for (index, ch) in input.char_indices() {
        let kind = if ch.is_ascii_alphanumeric() || ch == '_' {
            InlineKind::Word
        } else if ch.is_whitespace() {
            InlineKind::Space
        } else {
            InlineKind::Punct
        };

        match current_start {
            Some(start) if kind == current_kind && kind != InlineKind::Punct => {}
            Some(start) => {
                tokens.push(InlineToken {
                    start,
                    text: &input[start..index],
                });
                current_start = Some(index);
                current_kind = kind;
            }
            None => {
                current_start = Some(index);
                current_kind = kind;
            }
        }
    }

    if let Some(start) = current_start {
        tokens.push(InlineToken {
            start,
            text: &input[start..],
        });
    }

    tokens
}

fn collect_inline_ranges(
    source: &str,
    tokens: &[InlineToken<'_>],
    shared: &[bool],
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;

    for (token, is_shared) in tokens.iter().zip(shared.iter().copied()) {
        let is_significant = !token.text.trim().is_empty();
        if !is_shared && is_significant {
            start.get_or_insert(token.start);
        } else if let Some(range_start) = start.take() {
            ranges.push((range_start, token.start));
        }
    }

    if let Some(range_start) = start {
        ranges.push((range_start, source.len()));
    }

    ranges
}

fn fallback_inline_ranges(left: &str, right: &str) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    if left == right {
        return (Vec::new(), Vec::new());
    }

    let prefix = left
        .chars()
        .zip(right.chars())
        .take_while(|(left_char, right_char)| left_char == right_char)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();

    let mut left_suffix = left.len();
    let mut right_suffix = right.len();
    while left_suffix > prefix && right_suffix > prefix {
        let left_char = left[..left_suffix].chars().next_back().unwrap();
        let right_char = right[..right_suffix].chars().next_back().unwrap();
        if left_char != right_char {
            break;
        }
        left_suffix -= left_char.len_utf8();
        right_suffix -= right_char.len_utf8();
    }

    let left_range = if prefix < left_suffix {
        vec![(prefix, left_suffix)]
    } else {
        Vec::new()
    };
    let right_range = if prefix < right_suffix {
        vec![(prefix, right_suffix)]
    } else {
        Vec::new()
    };
    (left_range, right_range)
}

fn render_file(file: &MergedDiffFile) -> String {
    let file_id = file_dom_id(&file.path);
    let language = infer_language(&file.path);
    let hunks = if file.hunks.is_empty() {
        "<div class=\"file-empty\">No textual changes to display.</div>".to_string()
    } else {
        render_file_hunks(file, &file_id)
    };
    let open_attr = " open";

    format!(
        "<details id=\"{}\" class=\"file-card\" data-language=\"{}\" data-file-id=\"{}\" data-stage=\"{}\" data-search=\"{} {}\"{}><summary><span class=\"file-main\"><span class=\"file-path\">{}</span><span class=\"file-stage-meta\">{}</span></span><span class=\"file-stats\"><span class=\"added\">+{}</span><span class=\"removed\">-{}</span></span></summary><div class=\"file-body\"><div class=\"file-body-inner\">{}</div></div></details>",
        escape_html(&file_id),
        language,
        escape_html(&file_id),
        file.stage.dom_label(),
        escape_html(&file.path),
        escape_html(file.stage.meta_label()),
        open_attr,
        escape_html(&file.path),
        escape_html(file.stage.meta_label()),
        file.added,
        file.removed,
        hunks
    )
}

fn render_file_hunks(file: &MergedDiffFile, file_id: &str) -> String {
    let mut rendered = String::new();
    let mut previous_end = 0usize;

    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        let next_start = hunk_display_start(hunk);
        if next_start > previous_end.saturating_add(1) {
            rendered.push_str(&render_context_excerpt(
                file_id,
                &file.source_lines,
                previous_end.saturating_add(1),
                next_start - 1,
            ));
        }

        rendered.push_str(&render_hunk(file_id, hunk, hunk_index));
        previous_end = hunk_display_end(hunk).max(previous_end);
    }

    if file.source_lines.len() > previous_end {
        rendered.push_str(&render_context_excerpt(
            file_id,
            &file.source_lines,
            previous_end.saturating_add(1),
            file.source_lines.len(),
        ));
    }

    rendered
}

fn render_context_excerpt(
    file_id: &str,
    source_lines: &[String],
    start_line: usize,
    end_line: usize,
) -> String {
    if start_line == 0 || end_line < start_line {
        return String::new();
    }

    let total_lines = end_line - start_line + 1;
    let excerpt_rows = source_lines
        .iter()
        .enumerate()
        .skip(start_line - 1)
        .take(total_lines)
        .enumerate()
        .map(|(offset, (index, text))| {
            render_hidden_context_row(
                &DiffLine {
                    kind: DiffLineKind::Context,
                    old_line: Some(index + 1),
                    new_line: Some(index + 1),
                    text: text.clone(),
                    inline_ranges: Vec::new(),
                },
                offset,
            )
        })
        .collect::<Vec<_>>()
        .join("");

    if excerpt_rows.is_empty() {
        return String::new();
    }

    let excerpt_id = excerpt_dom_id(file_id, &format!("gap-{start_line}-{end_line}"));
    format!(
        "<div class=\"context-group context-group-file\" data-excerpt-id=\"{}\" data-excerpt-total=\"{}\">{}<div class=\"row row-gap row-gap-boundary\" data-role=\"excerpt-controls\"><div class=\"line gap-line\"><button class=\"gap-edge-button\" type=\"button\" data-role=\"excerpt-expand-top\" aria-label=\"Reveal more above\"><span class=\"gap-control-icon gap-control-icon-up\"></span></button><button class=\"gap-edge-button\" type=\"button\" data-role=\"excerpt-expand-bottom\" aria-label=\"Reveal more below\"><span class=\"gap-control-icon gap-control-icon-down\"></span></button></div><div class=\"code\"><span class=\"gap-label\" data-role=\"excerpt-label\">{} unchanged lines</span></div></div></div>",
        escape_html(&excerpt_id),
        total_lines,
        excerpt_rows,
        total_lines
    )
}

fn hunk_display_start(hunk: &MergedDiffHunk) -> usize {
    hunk.lines
        .iter()
        .filter_map(diff_line_number)
        .min()
        .unwrap_or(0)
}

fn hunk_display_end(hunk: &MergedDiffHunk) -> usize {
    hunk.lines
        .iter()
        .filter_map(diff_line_number)
        .max()
        .unwrap_or(0)
}

fn diff_line_number(line: &DiffLine) -> Option<usize> {
    line.new_line.or(line.old_line)
}

fn render_file_tree(files: &[MergedDiffFile]) -> String {
    let mut root = TreeDirectory::default();
    for file in files {
        insert_tree_file(&mut root, file);
    }
    let tree_html = if root.directories.is_empty() && root.files.is_empty() {
        String::from("<div class=\"tree-empty\">Nothing here</div>")
    } else {
        render_tree_directory_contents(&root, 0, "")
    };

    format!(
        "<div class=\"file-tree-shell\"><div class=\"file-tree-header\"><div class=\"file-tree-count\">{}</div></div><label class=\"file-tree-filter\"><input type=\"search\" data-role=\"file-filter\" placeholder=\"Filter files...\" spellcheck=\"false\"></label><div class=\"file-tree-groups\">{}</div></div>",
        change_label(files.len()),
        tree_html,
    )
}

fn file_dom_id(path: &str) -> String {
    format!("file-{}", slugify(path))
}

fn excerpt_dom_id(file_id: &str, key: &str) -> String {
    format!("excerpt-{}-{}", file_id, slugify(key))
}

fn hunk_dom_id(file_id: &str, index: usize) -> String {
    format!("hunk-{}-{}", file_id, index)
}

fn tree_dom_id(path: &str) -> String {
    format!("tree-{}", slugify(path))
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
    stage: FileStage,
}

fn insert_tree_file(root: &mut TreeDirectory, file: &MergedDiffFile) {
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
        file_id: file_dom_id(&file.path),
        stage: file.stage,
    });
}

fn render_tree_directory_contents(
    directory: &TreeDirectory,
    depth: usize,
    path_prefix: &str,
) -> String {
    let mut rendered = directory
        .directories
        .iter()
        .map(|(name, child)| render_tree_directory(name, child, depth, path_prefix))
        .collect::<Vec<_>>();

    let mut files = directory.files.iter().collect::<Vec<_>>();
    files.sort_by(|left, right| left.label.cmp(&right.label));
    rendered.extend(files.into_iter().map(|file| render_tree_file(file, depth)));
    rendered.join("")
}

fn render_tree_directory(
    name: &str,
    directory: &TreeDirectory,
    depth: usize,
    path_prefix: &str,
) -> String {
    let (label, path, directory) = flatten_tree_directory(name, directory, path_prefix);
    format!(
        "<details class=\"tree-group tree-dir\" data-tree-id=\"{}\" open><summary class=\"tree-row tree-row-dir\" style=\"--depth:{}\"><span class=\"tree-caret\">{}</span><span class=\"tree-icon tree-icon-folder\">{}</span><span class=\"tree-label\">{}</span></summary><div class=\"tree-children\">{}</div></details>",
        escape_html(&tree_dom_id(&path)),
        depth,
        chevron_icon(),
        folder_icon(),
        escape_html(&label),
        render_tree_directory_contents(directory, depth + 1, &path)
    )
}

fn flatten_tree_directory<'a>(
    name: &str,
    directory: &'a TreeDirectory,
    path_prefix: &str,
) -> (String, String, &'a TreeDirectory) {
    let mut label = name.to_string();
    let mut path = if path_prefix.is_empty() {
        name.to_string()
    } else {
        format!("{path_prefix}/{name}")
    };
    let mut current = directory;

    while current.files.is_empty() && current.directories.len() == 1 {
        let Some((child_name, child_directory)) = current.directories.iter().next() else {
            break;
        };
        label.push('/');
        label.push_str(child_name);
        path.push('/');
        path.push_str(child_name);
        current = child_directory;
    }

    (label, path, current)
}

fn render_tree_file(file: &TreeFileEntry, depth: usize) -> String {
    format!(
        "<button class=\"tree-row tree-file\" type=\"button\" style=\"--depth:{}\" data-file-target=\"{}\" data-filter-text=\"{} {}\"><span class=\"tree-row-spacer\"></span><span class=\"tree-icon tree-icon-file\">{}</span><span class=\"tree-label\">{}</span><span class=\"tree-stage-dot {}\"></span></button>",
        depth,
        escape_html(&file.file_id),
        escape_html(&file.path),
        escape_html(file.stage.meta_label()),
        file_icon(),
        escape_html(&file.label),
        file.stage.tree_class(),
    )
}

fn change_label(count: usize) -> String {
    if count == 1 {
        String::from("1 Change")
    } else {
        format!("{count} Changes")
    }
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string())
}

fn render_hunk(file_id: &str, hunk: &MergedDiffHunk, hunk_index: usize) -> String {
    let rows = render_hunk_rows(file_id, hunk_index, &hunk.lines);
    format!(
        "<div class=\"hunk\" data-hunk-id=\"{}\" data-file-id=\"{}\"><div class=\"diff-grid\">{}</div></div>",
        escape_html(&hunk_dom_id(file_id, hunk_index)),
        escape_html(file_id),
        rows,
    )
}

fn render_hunk_rows(file_id: &str, hunk_index: usize, lines: &[DiffLine]) -> String {
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
            let hidden_rows = hidden_lines
                .iter()
                .enumerate()
                .map(|(offset, line)| render_hidden_context_row(line, offset))
                .collect::<Vec<_>>()
                .join("");
            let hidden_start = hidden_lines
                .first()
                .and_then(diff_line_number)
                .unwrap_or_default();
            let hidden_end = hidden_lines
                .last()
                .and_then(diff_line_number)
                .unwrap_or_default();
            let excerpt_id = excerpt_dom_id(
                file_id,
                &format!("hunk-{hunk_index}-{hidden_start}-{hidden_end}"),
            );
            rendered.push_str(&format!(
                "<div class=\"context-group\" data-excerpt-id=\"{}\" data-excerpt-total=\"{}\">{}<div class=\"row row-gap row-gap-boundary\" data-role=\"excerpt-controls\"><div class=\"line gap-line\"><button class=\"gap-edge-button\" type=\"button\" data-role=\"excerpt-expand-top\" aria-label=\"Reveal more above\"><span class=\"gap-control-icon gap-control-icon-up\"></span></button><button class=\"gap-edge-button\" type=\"button\" data-role=\"excerpt-expand-bottom\" aria-label=\"Reveal more below\"><span class=\"gap-control-icon gap-control-icon-down\"></span></button></div><div class=\"code\"><span class=\"gap-label\" data-role=\"excerpt-label\">{} unchanged lines</span></div></div></div>",
                escape_html(&excerpt_id),
                hidden_lines.len(),
                hidden_rows,
                hidden_lines.len()
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

    let inline_ranges = if line.inline_ranges.is_empty() {
        String::new()
    } else {
        line.inline_ranges
            .iter()
            .map(|(start, end)| format!("{start}:{end}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let inline_attrs = if inline_ranges.is_empty() {
        String::new()
    } else {
        format!(
            " data-inline-ranges=\"{}\" data-inline-kind=\"{}\"",
            escape_html(&inline_ranges),
            match line.kind {
                DiffLineKind::Added => "added",
                DiffLineKind::Removed => "removed",
                _ => "",
            }
        )
    };

    format!(
        "<div class=\"row {}\"><div class=\"line\">{}</div><div class=\"code\" data-highlight=\"1\"><span class=\"code-content\" data-source=\"{}\"{}>{}</span></div></div>",
        row_class,
        escape_html(&display_line),
        escape_html(&line.text),
        inline_attrs,
        escape_html(&line.text)
    )
}

fn render_hidden_context_row(line: &DiffLine, index: usize) -> String {
    format!(
        "<div class=\"context-expand-row\" data-row-index=\"{}\">{}</div>",
        index,
        render_row(line)
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
  --diff-zoom: 0.88;
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
  display: block;
}
.diff-zoom-surface {
  display: grid;
  grid-template-columns: 0 minmax(0, 1fr);
  gap: 18px;
  align-items: start;
  zoom: var(--diff-zoom);
}
body.tree-open .diff-zoom-surface {
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
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 6px 8px;
  margin-bottom: 4px;
  padding: 0 0 6px;
  background: linear-gradient(180deg, rgba(15, 16, 17, 0.98) 0%, rgba(15, 16, 17, 0.92) 72%, rgba(15, 16, 17, 0) 100%);
}
.view-toolbar-meta {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}
.toolbar-total {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.03em;
}
.toolbar-total-added {
  color: #42d17f;
}
.toolbar-total-removed {
  color: #ff5f61;
}
.view-toolbar-search {
  flex: 1 1 200px;
  min-width: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
}
.toolbar-search {
  flex: 1 1 200px;
  max-width: 320px;
  min-width: 0;
  height: 28px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px;
  border-radius: 6px;
  border: 1px solid rgba(255,255,255,0.06);
  background: rgba(24, 25, 27, 0.72);
  color: #8f97a2;
}
.toolbar-search:focus-within {
  border-color: rgba(104, 156, 255, 0.18);
  background: rgba(28, 29, 32, 0.9);
}
.toolbar-search-icon {
  width: 12px;
  height: 12px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: inherit;
}
.toolbar-search-icon svg {
  width: 12px;
  height: 12px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.toolbar-search input {
  flex: 1 1 auto;
  min-width: 0;
  border: 0;
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 11px;
  line-height: 1.1;
}
.toolbar-search input:focus {
  outline: none;
}
.toolbar-search input::placeholder {
  color: #8f97a2;
}
.toolbar-search-count {
  min-width: 36px;
  text-align: right;
  color: #8f97a2;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.04em;
}
.view-toolbar-actions {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.toolbar-btn {
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: #8f97a2;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}
.toolbar-btn:hover {
  background: rgba(255,255,255,0.04);
  color: #cfd5dc;
}
.toolbar-btn:disabled {
  opacity: 0.42;
  cursor: default;
}
.toolbar-btn:disabled:hover {
  background: transparent;
}
.toolbar-btn.is-active {
  background: rgba(255,255,255,0.04);
  color: #9ec6ff;
}
.toolbar-btn svg {
  width: 14px;
  height: 14px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.9;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.file-tree-panel {
  position: sticky;
  top: 12px;
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
  padding: 2px 0 0;
}
.file-tree-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  padding: 0;
}
.file-tree-count {
  font-size: 13px;
  font-weight: 700;
  color: #c8ced6;
}
.file-tree-filter {
  display: block;
  margin-bottom: 10px;
}
.file-tree-filter input {
  width: 100%;
  padding: 7px 9px;
  border: 1px solid rgba(255,255,255,0.06);
  border-radius: 6px;
  background: rgba(24, 25, 27, 0.72);
  color: var(--text);
  font: inherit;
  font-size: 12px;
  line-height: 1.2;
  transition: border-color 120ms ease, background 120ms ease, box-shadow 120ms ease;
}
.file-tree-filter input:focus {
  outline: none;
  border-color: rgba(104, 156, 255, 0.18);
  background: rgba(28, 29, 32, 0.9);
  box-shadow: none;
}
.file-tree-filter input::placeholder {
  color: #a0a5ad;
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
  --tree-indent-step: 10px;
  position: relative;
  width: 100%;
  min-height: 24px;
  display: grid;
  grid-template-columns: 10px 12px minmax(0, 1fr) auto;
  align-items: center;
  column-gap: 6px;
  margin-bottom: 0;
  padding: 2px 6px 2px calc(4px + (var(--depth) * var(--tree-indent-step)));
  border-radius: 0;
  border: 0;
  background: transparent;
  color: var(--text);
  text-align: left;
}
.tree-row::before {
  content: "";
  position: absolute;
  left: calc(8px + (var(--depth) * var(--tree-indent-step)));
  top: 0;
  bottom: 0;
  width: 1px;
  background: rgba(255,255,255,0.05);
}
.tree-row::after {
  content: "";
  position: absolute;
  left: calc(8px + (var(--depth) * var(--tree-indent-step)));
  top: 50%;
  width: 4px;
  height: 1px;
  background: rgba(255,255,255,0.05);
}
.tree-row-dir {
  cursor: pointer;
  color: #aeb5bf;
  font-weight: 600;
}
.tree-row-dir:hover {
  background: rgba(255,255,255,0.02);
}
.tree-file {
  cursor: pointer;
  transition: background 120ms ease, box-shadow 120ms ease;
}
.tree-file:hover {
  background: rgba(255,255,255,0.025);
}
.tree-file.is-active {
  background: rgba(255,255,255,0.03);
  box-shadow: inset 2px 0 0 rgba(104, 156, 255, 0.75);
}
.tree-caret,
.tree-row-spacer {
  position: relative;
  z-index: 1;
  width: 10px;
  height: 10px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #c8ccd3;
}
.tree-caret svg {
  width: 10px;
  height: 10px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
  transform: rotate(-90deg);
  transition: transform 120ms ease;
}
.tree-group[open] > summary .tree-caret svg {
  transform: rotate(0deg);
}
.tree-row-spacer::before {
  content: "";
}
.tree-icon {
  position: relative;
  z-index: 1;
  width: 12px;
  height: 12px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #bcc3cc;
}
.tree-icon svg {
  width: 12px;
  height: 12px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.tree-label {
  min-width: 0;
  font-size: 11px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tree-stage-dot {
  width: 5px;
  height: 5px;
  border-radius: 999px;
}
.tree-stage-working {
  background: #7ee7a7;
}
.tree-stage-index {
  background: #9ec6ff;
}
.tree-stage-both {
  background: linear-gradient(90deg, #7ee7a7 0 50%, #9ec6ff 50% 100%);
}
.tree-empty {
  padding: 4px 6px 4px 18px;
  color: #737b85;
  font-size: 11px;
}
.hero {
  margin-bottom: 12px;
  padding: 2px 0 0;
}
.hero-label {
  display: none;
}
.hero h1 {
  margin: 0 0 4px;
  font-size: 13px;
  line-height: 1.25;
  font-weight: 700;
  color: #bcc3cc;
}
.hero p {
  margin: 0;
  color: #7f8791;
  font-size: 11px;
  word-break: break-all;
}
.hero .error {
  color: #ff9b9b;
}
.diff-section {
  margin-bottom: 10px;
}
.section-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 6px;
  padding: 0 0 4px;
  background: transparent;
  border: 0;
  border-radius: 0;
}
.section-title {
  font-size: 11px;
  font-weight: 700;
  color: #aeb5bf;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.section-count {
  min-width: 0;
  padding: 0;
  text-align: center;
  background: transparent;
  color: #6f7781;
  font-size: 11px;
  font-weight: 700;
}
.empty-state, .file-empty {
  padding: 6px 0;
  border: 0;
  border-radius: 0;
  background: transparent;
  color: var(--muted);
  font-size: 12px;
}
.file-card {
  margin-bottom: 8px;
  border: 1px solid rgba(255,255,255,0.04);
  border-radius: 4px;
  background: rgba(21, 22, 24, 0.6);
  overflow: hidden;
}
.file-card.is-active {
  border-color: rgba(104, 156, 255, 0.12);
}
.file-card > summary {
  list-style: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 10px;
  cursor: pointer;
  background: rgba(24, 25, 27, 0.55);
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
  gap: 8px;
  min-width: 0;
}
.file-path {
  font-size: 12px;
  font-weight: 700;
  word-break: break-word;
}
.file-stage-meta {
  flex: none;
  color: #6f7781;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
.file-stats {
  display: inline-flex;
  gap: 6px;
  font-size: 11px;
  white-space: nowrap;
}
.added { color: #42d17f; }
.removed { color: #ff5f61; }
.file-body {
  padding: 0 0 6px;
  overflow-x: auto;
}
.file-body-inner {
  min-width: max-content;
}
.hunk {
  margin-top: 6px;
  padding-top: 4px;
  border-top: 1px solid rgba(255,255,255,0.05);
  border-radius: 0;
  overflow: visible;
  background: transparent;
  scroll-margin-top: 84px;
}
.hunk:first-child {
  margin-top: 0;
  padding-top: 0;
  border-top: 0;
}
.diff-grid {
  display: block;
  min-width: max-content;
}
.row {
  display: grid;
  grid-template-columns: 56px minmax(0, 1fr);
  align-items: stretch;
  min-height: 24px;
}
.row .line,
.row .code {
  padding: 0 10px;
}
.row .line {
  color: #7f8791;
  text-align: right;
  border-right: 1px solid rgba(55, 57, 62, 0.65);
  line-height: 24px;
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
  display: flex;
  align-items: center;
  position: relative;
  white-space: pre;
  line-height: 24px;
  word-break: normal;
  overflow-wrap: normal;
}
.row .code > * {
  position: relative;
  z-index: 1;
}
.code-content,
.code-content * {
  line-height: inherit;
}
.row .code::selection,
.row .code *::selection {
  background: transparent;
  color: inherit;
}
.row.row-selected .code::before {
  content: "";
  position: absolute;
  inset: 0;
  background: rgba(92, 146, 214, 0.58);
  pointer-events: none;
}
.inline-diff {
  border-radius: 3px;
  box-decoration-break: clone;
  -webkit-box-decoration-break: clone;
}
.inline-diff-added {
  background: rgba(71, 214, 114, 0.22);
}
.inline-diff-removed {
  background: rgba(255, 96, 96, 0.24);
}
.search-hit {
  background: rgba(255, 219, 87, 0.22);
  box-decoration-break: clone;
  -webkit-box-decoration-break: clone;
}
.search-hit.is-current {
  background: rgba(255, 219, 87, 0.52);
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
  display: flex;
  flex-direction: column;
  position: relative;
}
.gap-line {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: visible;
}
.gap-edge-button {
  width: 30px;
  height: 20px;
  border: 0;
  background: rgba(255,255,255,0.05);
  color: #8f99a5;
  font: inherit;
}
.gap-edge-button:first-child {
  border-radius: 8px 8px 0 0;
}
.gap-edge-button:last-child {
  border-radius: 0 0 8px 8px;
}
.gap-edge-button:hover,
.gap-edge-button:focus-visible {
  background: rgba(255,255,255,0.09);
  color: #c4ccd5;
  outline: none;
}
.gap-control-icon {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  font-size: 13px;
  line-height: 1;
}
.gap-control-icon::before {
  content: "";
}
.gap-control-icon-up::before {
  content: "⌃";
}
.gap-control-icon-down::before {
  content: "⌄";
}
.gap-popover {
  position: absolute;
  left: 44px;
  top: 50%;
  transform: translateY(-50%);
  display: inline-flex;
  align-items: center;
  gap: 14px;
  padding: 9px 14px;
  border: 1px solid rgba(255,255,255,0.06);
  border-radius: 14px;
  background: rgba(56, 60, 69, 0.88);
  box-shadow: 0 8px 20px rgba(0,0,0,0.22);
  color: #dbe2ea;
  white-space: nowrap;
  pointer-events: none;
  opacity: 0.9;
  z-index: 10;
  transition: opacity 120ms ease, background 120ms ease, border-color 120ms ease;
}
.row-gap:hover .gap-popover,
.context-group > summary:focus-visible .gap-popover {
  opacity: 1;
  background: rgba(62, 66, 76, 0.96);
  border-color: rgba(255,255,255,0.1);
}
.gap-popover-title {
  font-size: 11px;
  font-weight: 700;
}
.gap-popover-shortcut {
  color: #abb3be;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.03em;
}
.context-group[open] .gap-popover {
  opacity: 0.72;
}
.row-gap .code {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 6px 12px;
  border-radius: 0;
  margin: 0;
  background: rgba(255,255,255,0.02);
  transition: background 120ms ease, color 120ms ease;
}
.row-gap:hover .code {
  background: rgba(255,255,255,0.04);
}
.row-gap-boundary {
  width: 100%;
  border: 0;
  padding: 0;
  text-align: left;
  font: inherit;
}
.gap-action {
  flex: none;
  display: inline-flex;
  align-items: center;
  min-height: 22px;
  padding: 0 10px;
  border-radius: 999px;
  background: rgba(104, 156, 255, 0.1);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: #9ec6ff;
}
.row-gap:hover .gap-action {
  background: rgba(104, 156, 255, 0.18);
}
.context-expand-row {
  display: none;
}
.context-expand-row.is-visible {
  display: block;
}
.row-gap-boundary .code {
  justify-content: flex-start;
}
.filter-hidden {
  display: none !important;
}
"#
}

fn document_js() -> &'static str {
    r###"
(function () {
  const EXCERPT_CHUNK = 10;
  const SEARCH_INPUT_DEBOUNCE_MS = 60;
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

  function parseInlineRanges(value) {
    if (!value) return [];
    return value.split(',').map((part) => {
      const [start, end] = part.split(':').map((piece) => Number(piece));
      return Number.isFinite(start) && Number.isFinite(end) ? [start, end] : null;
    }).filter(Boolean);
  }

  function findSearchRanges(input, query) {
    if (!query) return [];

    const needle = query.toLowerCase();
    const haystack = input.toLowerCase();
    const ranges = [];
    let cursor = 0;

    while (cursor <= haystack.length - needle.length) {
      const matchIndex = haystack.indexOf(needle, cursor);
      if (matchIndex === -1) break;
      ranges.push([matchIndex, matchIndex + needle.length]);
      cursor = matchIndex + Math.max(needle.length, 1);
    }

    return ranges;
  }

  function clampRanges(inputLength, ranges) {
    return ranges
      .map(([start, end]) => [Math.max(0, start), Math.min(inputLength, end)])
      .filter(([start, end]) => end > start);
  }

  function rangeIntersects(ranges, start, end) {
    return ranges.some(([rangeStart, rangeEnd]) => rangeStart < end && rangeEnd > start);
  }

  function highlightDecoratedLine(input, language, inlineRanges, inlineKind, searchRanges) {
    const normalizedInlineRanges = clampRanges(input.length, inlineRanges);
    const normalizedSearchRanges = clampRanges(input.length, searchRanges);
    if (!normalizedInlineRanges.length && !normalizedSearchRanges.length) {
      return highlightLine(input, language);
    }

    const boundaries = new Set([0, input.length]);
    normalizedInlineRanges.forEach(([start, end]) => {
      boundaries.add(start);
      boundaries.add(end);
    });
    normalizedSearchRanges.forEach(([start, end]) => {
      boundaries.add(start);
      boundaries.add(end);
    });

    const sorted = Array.from(boundaries).sort((left, right) => left - right);
    let html = "";
    for (let index = 0; index < sorted.length - 1; index += 1) {
      const start = sorted[index];
      const end = sorted[index + 1];
      if (end <= start) continue;

      const segment = input.slice(start, end);
      let segmentHtml = highlightLine(segment, language);

      if (inlineKind && rangeIntersects(normalizedInlineRanges, start, end)) {
        segmentHtml = `<span class="inline-diff inline-diff-${inlineKind}">${segmentHtml}</span>`;
      }

      if (rangeIntersects(normalizedSearchRanges, start, end)) {
        segmentHtml = `<span class="search-hit">${segmentHtml}</span>`;
      }

      html += segmentHtml;
    }

    return html;
  }

  const body = document.body;
  const treeButton = document.querySelector('[data-action="toggle-tree"]');
  const searchInput = document.querySelector('[data-role="content-search"]');
  const prevMatchButton = document.querySelector('[data-action="prev-match"]');
  const nextMatchButton = document.querySelector('[data-action="next-match"]');
  const searchCount = document.querySelector('[data-role="search-count"]');
  const prevFileButton = document.querySelector('[data-action="prev-file"]');
  const nextFileButton = document.querySelector('[data-action="next-file"]');
  const prevHunkButton = document.querySelector('[data-action="prev-hunk"]');
  const nextHunkButton = document.querySelector('[data-action="next-hunk"]');
  const collapseButton = document.querySelector('[data-action="toggle-collapse"]');
  const fullscreenButton = document.querySelector('[data-action="toggle-fullscreen"]');
  const closeButton = document.querySelector('[data-action="close-diff"]');
  const filterInput = document.querySelector('[data-role="file-filter"]');
  const fileCards = Array.from(document.querySelectorAll('.file-card'));
  const treeFiles = Array.from(document.querySelectorAll('.tree-file'));
  const treeGroups = Array.from(document.querySelectorAll('.tree-group'));
  const highlightedCodeNodes = Array.from(document.querySelectorAll('.code[data-highlight="1"] .code-content'));
  const initialState = window.__NOT_TERMINAL_DIFF_INITIAL_STATE__ || null;
  let currentSearchQuery = "";
  let searchMatches = [];
  let activeSearchMatchIndex = -1;
  let contentSearchDebounce = 0;

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

  let pendingFocusTarget = null;

  function beginTextInput(target) {
    pendingFocusTarget = target;
    postNativeAction('enable-text-input');
    window.setTimeout(() => {
      if (target && typeof target.focus === 'function') {
        target.focus();
        if (typeof target.select === 'function') {
          target.select();
        }
      }
    }, 0);
  }

  window.__WV_REFOCUS__ = function () {
    const target = pendingFocusTarget;
    pendingFocusTarget = null;
    if (target && typeof target.focus === 'function') {
      target.focus();
      if (typeof target.select === 'function') {
        target.select();
      }
    }
  };

  function endTextInput() {
    postNativeAction('disable-text-input');
  }

  function visibleCards() {
    return fileCards.filter((card) => !card.classList.contains('filter-hidden'));
  }

  function activeCard() {
    const activeId = body.dataset.activeFile || "";
    return fileCards.find((card) => card.dataset.fileId === activeId) || null;
  }

  function markActiveFile(fileId) {
    body.dataset.activeFile = fileId;
    fileCards.forEach((candidate) => {
      candidate.classList.toggle('is-active', candidate.dataset.fileId === fileId);
    });
    treeFiles.forEach((item) => {
      const isActive = item.dataset.fileTarget === fileId;
      item.classList.toggle('is-active', isActive);
      if (isActive) {
        item.scrollIntoView({ block: 'nearest', inline: 'nearest' });
      }
    });
  }

  function visibleNavigableCards() {
    return visibleCards().filter((card) => !card.classList.contains('filter-hidden'));
  }

  function visibleNavigableHunks() {
    return Array.from(document.querySelectorAll('.hunk')).filter((hunk) => {
      const card = hunk.closest('.file-card');
      return !!card && !card.classList.contains('filter-hidden') && card.open;
    });
  }

  function firstPreferredCard() {
    return visibleCards().find((card) => card.dataset.stage !== 'index')
      || visibleCards()[0]
      || fileCards.find((card) => card.dataset.stage !== 'index')
      || fileCards[0]
      || null;
  }

  function setActiveFile(fileId, options = {}) {
    const card = fileCards.find((candidate) => candidate.dataset.fileId === fileId);
    if (!card) return;

    markActiveFile(fileId);

    if (options.open !== false && !card.open) {
      card.open = true;
    }

    if (options.scroll) {
      card.scrollIntoView({ block: 'start', behavior: 'auto' });
    }

    syncToolbar();
  }

  let scrollSelectionFrame = 0;
  function syncActiveFileFromScroll() {
    if (scrollSelectionFrame) return;
    scrollSelectionFrame = window.requestAnimationFrame(() => {
      scrollSelectionFrame = 0;
      const candidates = visibleCards().filter((card) => {
        const rect = card.getBoundingClientRect();
        return rect.bottom > 56 && rect.top < window.innerHeight - 40;
      });
      const pool = candidates.length ? candidates : visibleCards();
      if (!pool.length) return;

      let best = pool[0];
      let bestDistance = Number.POSITIVE_INFINITY;
      pool.forEach((card) => {
        const rect = card.getBoundingClientRect();
        const distance = Math.abs(rect.top - 72);
        if (distance < bestDistance) {
          best = card;
          bestDistance = distance;
        }
      });

      if (best.dataset.fileId && best.dataset.fileId !== body.dataset.activeFile) {
        markActiveFile(best.dataset.fileId);
      }
    });
  }

  function syncToolbar() {
    const treeOpen = body.classList.contains('tree-open');
    const splitZoomed = fullscreenButton?.classList.contains('is-active') || false;
    const navigableCards = visibleNavigableCards();
    const navigableHunks = visibleNavigableHunks();
    const active = activeCard();
    const activeIndex = active ? navigableCards.indexOf(active) : -1;
    const hunkIndex = currentHunkIndex(navigableHunks);
    const allExpanded = visibleNavigableCards().length > 0
      && visibleNavigableCards().every((card) => card.open);

    if (treeButton) {
      treeButton.classList.toggle('is-active', treeOpen);
      treeButton.title = treeOpen ? 'Hide file tree' : 'Show file tree';
      treeButton.setAttribute('aria-label', treeButton.title);
    }

    if (prevFileButton) {
      prevFileButton.disabled = navigableCards.length <= 1 || activeIndex <= 0;
      prevFileButton.title = 'Previous file';
      prevFileButton.setAttribute('aria-label', 'Previous file');
    }

    if (nextFileButton) {
      nextFileButton.disabled = navigableCards.length <= 1
        || activeIndex === -1
        || activeIndex >= navigableCards.length - 1;
      nextFileButton.title = 'Next file';
      nextFileButton.setAttribute('aria-label', 'Next file');
    }

    if (prevHunkButton) {
      prevHunkButton.disabled = navigableHunks.length <= 1 || hunkIndex <= 0;
      prevHunkButton.title = 'Previous change';
      prevHunkButton.setAttribute('aria-label', 'Previous change');
    }

    if (nextHunkButton) {
      nextHunkButton.disabled = navigableHunks.length <= 1
        || hunkIndex === -1
        || hunkIndex >= navigableHunks.length - 1;
      nextHunkButton.title = 'Next change';
      nextHunkButton.setAttribute('aria-label', 'Next change');
    }

    if (collapseButton) {
      collapseButton.classList.toggle('is-active', allExpanded);
      collapseButton.title = allExpanded ? 'Collapse files' : 'Expand files';
      collapseButton.setAttribute('aria-label', collapseButton.title);
    }

    if (fullscreenButton) {
      fullscreenButton.title = splitZoomed ? 'Exit fullscreen' : 'Enter fullscreen';
      fullscreenButton.setAttribute('aria-label', fullscreenButton.title);
    }

    if (prevMatchButton) {
      prevMatchButton.disabled = searchMatches.length <= 1;
      prevMatchButton.title = 'Previous match';
      prevMatchButton.setAttribute('aria-label', 'Previous match');
    }

    if (nextMatchButton) {
      nextMatchButton.disabled = searchMatches.length <= 1;
      nextMatchButton.title = 'Next match';
      nextMatchButton.setAttribute('aria-label', 'Next match');
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

  function closeDiff() {
    postNativeAction('toggle-diff-view');
  }

  function currentHunkIndex(hunks = visibleNavigableHunks()) {
    if (!hunks.length) return -1;

    let bestIndex = 0;
    let bestDistance = Number.POSITIVE_INFINITY;
    hunks.forEach((hunk, index) => {
      const rect = hunk.getBoundingClientRect();
      const distance = Math.abs(rect.top - 96);
      if (distance < bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    });
    return bestIndex;
  }

  function moveFile(offset) {
    const cards = visibleNavigableCards();
    if (!cards.length) return;

    const current = activeCard();
    const currentIndex = current ? cards.indexOf(current) : -1;
    const fallbackIndex = currentIndex === -1 ? 0 : currentIndex;
    const nextIndex = Math.min(cards.length - 1, Math.max(0, fallbackIndex + offset));
    const nextCard = cards[nextIndex];
    if (!nextCard) return;
    setActiveFile(nextCard.dataset.fileId || '', { scroll: true });
    syncToolbar();
  }

  function moveHunk(offset) {
    const hunks = visibleNavigableHunks();
    if (!hunks.length) return;

    const currentIndex = currentHunkIndex(hunks);
    const fallbackIndex = currentIndex === -1 ? 0 : currentIndex;
    const nextIndex = Math.min(hunks.length - 1, Math.max(0, fallbackIndex + offset));
    const nextHunk = hunks[nextIndex];
    if (!nextHunk) return;

    const fileId = nextHunk.dataset.fileId || "";
    if (fileId) {
      setActiveFile(fileId, { scroll: false, open: true });
    }

    nextHunk.scrollIntoView({ block: 'start', behavior: 'auto' });
    syncToolbar();
  }

  function toggleAllFiles() {
    const cards = visibleNavigableCards();
    if (!cards.length) return;
    const shouldExpand = cards.some((card) => !card.open);
    cards.forEach((card) => {
      card.open = shouldExpand;
    });
    syncToolbar();
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

    syncSearchResults(false);
    syncToolbar();
  }

  function syncSearchResults(scrollToCurrent = false) {
    searchMatches = Array.from(document.querySelectorAll('.search-hit')).filter((match) => {
      const card = match.closest('.file-card');
      return !card || !card.classList.contains('filter-hidden');
    });

    if (!currentSearchQuery || searchMatches.length === 0) {
      activeSearchMatchIndex = -1;
    } else if (activeSearchMatchIndex < 0 || activeSearchMatchIndex >= searchMatches.length) {
      activeSearchMatchIndex = 0;
    }

    searchMatches.forEach((match, index) => {
      match.classList.toggle('is-current', index === activeSearchMatchIndex);
    });

    if (searchCount) {
      searchCount.textContent =
        currentSearchQuery && searchMatches.length
          ? `${activeSearchMatchIndex + 1}/${searchMatches.length}`
          : '0';
    }

    if (scrollToCurrent && activeSearchMatchIndex >= 0) {
      const current = searchMatches[activeSearchMatchIndex];
      const card = current?.closest('.file-card');
      if (card?.dataset.fileId) {
        setActiveFile(card.dataset.fileId, { scroll: false, open: true });
      }
      current?.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'auto' });
    }

    syncToolbar();
  }

  function syncSearchAwareExcerpts() {
    document.querySelectorAll('.context-group').forEach((group) => {
      const hasSearchMatch = !!(currentSearchQuery && group.querySelector('.search-hit'));
      group.dataset.excerptSearchRevealAll = hasSearchMatch ? '1' : '0';
      updateExcerpt(group);
    });
  }

  function initializeHighlightedCode() {
    highlightedCodeNodes.forEach((node) => {
      const language = languageFor(node);
      const source = node.dataset.source || node.textContent || "";
      const ranges = parseInlineRanges(node.dataset.inlineRanges || "");
      const kind = node.dataset.inlineKind || "";
      const baseHtml = highlightDecoratedLine(source, language, ranges, kind, []);

      node.__diffLanguage = language;
      node.__diffSource = source;
      node.__diffSourceLower = source.toLowerCase();
      node.__diffInlineRanges = ranges;
      node.__diffInlineKind = kind;
      node.__diffBaseHtml = baseHtml;
      node.__diffRenderedQuery = "";
      node.__diffHasSearchHit = false;
      node.innerHTML = baseHtml;
    });
  }

  function renderHighlightedCode() {
    highlightedCodeNodes.forEach((node) => {
      const source = node.__diffSource || node.dataset.source || node.textContent || "";
      const sourceLower = node.__diffSourceLower || source.toLowerCase();
      const baseHtml = node.__diffBaseHtml || "";
      const language = node.__diffLanguage || languageFor(node);
      const ranges = node.__diffInlineRanges || parseInlineRanges(node.dataset.inlineRanges || "");
      const kind = node.__diffInlineKind || node.dataset.inlineKind || "";
      const hasSearchHit = !!currentSearchQuery && sourceLower.includes(currentSearchQuery);

      if (!currentSearchQuery) {
        if (node.__diffRenderedQuery !== "") {
          node.innerHTML = baseHtml;
          node.__diffRenderedQuery = "";
          node.__diffHasSearchHit = false;
        }
        return;
      }

      if (!hasSearchHit) {
        if (node.__diffHasSearchHit || node.__diffRenderedQuery !== "") {
          node.innerHTML = baseHtml;
          node.__diffRenderedQuery = "";
          node.__diffHasSearchHit = false;
        }
        return;
      }

      if (node.__diffRenderedQuery === currentSearchQuery && node.__diffHasSearchHit) {
        return;
      }

      const searchRanges = findSearchRanges(source, currentSearchQuery);
      node.innerHTML = highlightDecoratedLine(source, language, ranges, kind, searchRanges);
      node.__diffRenderedQuery = currentSearchQuery;
      node.__diffHasSearchHit = true;
    });

    if (currentSearchQuery) {
      fileCards.forEach((card) => {
        if (card.querySelector('.search-hit')) {
          card.open = true;
        }
      });
    }

    syncSearchAwareExcerpts();
    syncSearchResults(false);
  }

  function applyContentSearch(scrollToCurrent = false) {
    currentSearchQuery = (searchInput?.value || '').trim().toLowerCase();
    if (!currentSearchQuery) {
      activeSearchMatchIndex = -1;
    } else if (scrollToCurrent) {
      activeSearchMatchIndex = 0;
    }
    renderHighlightedCode();
    syncSearchResults(scrollToCurrent);
  }

  function scheduleContentSearch(scrollToCurrent = false) {
    if (contentSearchDebounce) {
      window.clearTimeout(contentSearchDebounce);
    }
    contentSearchDebounce = window.setTimeout(() => {
      contentSearchDebounce = 0;
      applyContentSearch(scrollToCurrent);
    }, SEARCH_INPUT_DEBOUNCE_MS);
  }

  function moveSearchMatch(offset) {
    if (!searchMatches.length) return;
    if (activeSearchMatchIndex < 0) {
      activeSearchMatchIndex = 0;
    } else {
      activeSearchMatchIndex =
        (activeSearchMatchIndex + offset + searchMatches.length) % searchMatches.length;
    }
    syncSearchResults(true);
  }

  function captureDiffState() {
    const excerpts = Array.from(document.querySelectorAll('.context-group[data-excerpt-id]')).map((group) => ({
      id: group.dataset.excerptId || "",
      head: Number(group.dataset.excerptHead || 0),
      tail: Number(group.dataset.excerptTail || 0),
    }));

    return JSON.stringify({
      scrollY: window.scrollY,
      diffZoom: Number(body.dataset.diffZoom || 1),
      treeOpen: body.classList.contains('tree-open'),
      splitZoomed: fullscreenButton?.classList.contains('is-active') || false,
      filterValue: filterInput?.value || "",
      contentSearchValue: searchInput?.value || "",
      activeSearchMatchIndex,
      activeFile: body.dataset.activeFile || "",
      openFiles: fileCards.filter((card) => card.open).map((card) => card.dataset.fileId || ""),
      openTreeGroups: treeGroups.filter((group) => group.open).map((group) => group.dataset.treeId || ""),
      excerpts,
    });
  }

  function restoreDiffState(state) {
    if (!state || typeof state !== 'object') return;

    if (Number.isFinite(state.diffZoom)) {
      setDiffZoom(Number(state.diffZoom));
    }

    body.classList.toggle('tree-open', !!state.treeOpen);
    fullscreenButton?.classList.toggle('is-active', !!state.splitZoomed);

    if (filterInput && typeof state.filterValue === 'string') {
      filterInput.value = state.filterValue;
      applyFilter();
    }

    if (searchInput && typeof state.contentSearchValue === 'string') {
      searchInput.value = state.contentSearchValue;
      currentSearchQuery = state.contentSearchValue.trim().toLowerCase();
    }

    // Note: openFiles and excerpts are intentionally NOT restored so that
    // the default "all expanded" state is preserved on every reload.

    if (Array.isArray(state.openTreeGroups)) {
      const openTreeGroups = new Set(state.openTreeGroups);
      treeGroups.forEach((group) => {
        group.open = openTreeGroups.has(group.dataset.treeId || "");
      });
    }

    if (typeof state.activeFile === 'string' && state.activeFile) {
      setActiveFile(state.activeFile, { scroll: false, open: false });
    }

    renderHighlightedCode();

    if (Number.isFinite(state.activeSearchMatchIndex)) {
      activeSearchMatchIndex = Number(state.activeSearchMatchIndex);
      syncSearchResults(false);
    }

    if (Number.isFinite(state.scrollY)) {
      window.scrollTo(0, Math.max(0, Number(state.scrollY)));
    }

    syncToolbar();
    syncActiveFileFromScroll();
  }

  const DEFAULT_DIFF_ZOOM = 0.88;
  const MIN_DIFF_ZOOM = 0.75;
  const MAX_DIFF_ZOOM = 1.75;
  const DIFF_ZOOM_STEP = 1 / 13;

  function clampDiffZoom(value) {
    return Math.min(MAX_DIFF_ZOOM, Math.max(MIN_DIFF_ZOOM, value));
  }

  function roundedDiffZoom(value) {
    return Math.round(value * 1000) / 1000;
  }

  function setDiffZoom(value) {
    const zoom = roundedDiffZoom(clampDiffZoom(value));
    body.dataset.diffZoom = String(zoom);
    document.documentElement.style.setProperty('--diff-zoom', String(zoom));
    return zoom;
  }

  function adjustDiffZoom(stepDelta) {
    const current = Number(body.dataset.diffZoom || 1);
    return setDiffZoom(current + (stepDelta * DIFF_ZOOM_STEP));
  }

  function resetDiffZoom() {
    return setDiffZoom(DEFAULT_DIFF_ZOOM);
  }

  window.__NOT_TERMINAL_DIFF_ADJUST_ZOOM__ = adjustDiffZoom;
  window.__NOT_TERMINAL_DIFF_RESET_ZOOM__ = resetDiffZoom;

  window.__NOT_TERMINAL_DIFF_CAPTURE_STATE__ = captureDiffState;

  const diffRows = Array.from(document.querySelectorAll('.row'));

  function clearRowSelection() {
    diffRows.forEach((row) => {
      row.classList.remove('row-selected');
    });
  }

  function syncCustomSelection() {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed || !body.contains(selection.anchorNode)) {
      clearRowSelection();
      return;
    }

    const range = selection.getRangeAt(0);
    const selectedRows = [];

    diffRows.forEach((row) => {
      const code = row.querySelector('.code');
      if (!code) return;

      let intersects = false;
      try {
        intersects = range.intersectsNode(code);
      } catch (_error) {
        intersects = false;
      }

      row.classList.toggle('row-selected', intersects);
      if (intersects) {
        selectedRows.push(row);
      }
    });

    if (selectedRows.length === 0) {
      clearRowSelection();
    }
  }

  treeButton?.addEventListener('click', toggleTree);
  prevMatchButton?.addEventListener('click', () => moveSearchMatch(-1));
  nextMatchButton?.addEventListener('click', () => moveSearchMatch(1));
  prevFileButton?.addEventListener('click', () => moveFile(-1));
  nextFileButton?.addEventListener('click', () => moveFile(1));
  prevHunkButton?.addEventListener('click', () => moveHunk(-1));
  nextHunkButton?.addEventListener('click', () => moveHunk(1));
  collapseButton?.addEventListener('click', toggleAllFiles);
  fullscreenButton?.addEventListener('click', toggleFullscreen);
  closeButton?.addEventListener('click', closeDiff);
  filterInput?.addEventListener('input', applyFilter);
  searchInput?.addEventListener('input', () => scheduleContentSearch(true));
  searchInput?.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      moveSearchMatch(event.shiftKey ? -1 : 1);
    }
  });
  [filterInput, searchInput].filter(Boolean).forEach((input) => {
    input.addEventListener('mousedown', () => beginTextInput(input));
    input.addEventListener('focus', () => beginTextInput(input));
    input.addEventListener('blur', () => {
      window.setTimeout(() => {
        const active = document.activeElement;
        if (active !== filterInput && active !== searchInput) {
          endTextInput();
        }
      }, 0);
    });
  });
  window.addEventListener('scroll', syncActiveFileFromScroll, { passive: true });
  document.addEventListener('selectionchange', syncCustomSelection);

  function isEditableTarget(target) {
    if (!(target instanceof Element)) return false;
    return target.closest('input, textarea, [contenteditable="true"], [contenteditable=""], [role="textbox"]') !== null;
  }

  // Only suppress the browser-style keys that cause the diff page to jump or
  // scroll unexpectedly. Command shortcuts like Cmd+C/Cmd+A/Cmd+Shift+D should
  // keep flowing through to AppKit/webview handling.
  window.addEventListener('keydown', (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }
    if (isEditableTarget(event.target)) {
      return;
    }

    const key = event.key;
    const shouldBlockScrollKey =
      key === ' ' ||
      key === 'PageUp' ||
      key === 'PageDown' ||
      key === 'Home' ||
      key === 'End';

    if (shouldBlockScrollKey) {
      event.preventDefault();
      event.stopPropagation();
    }
  }, true);

  function excerptRows(group) {
    return Array.from(group.querySelectorAll('.context-expand-row'));
  }

  function initializeExcerpt(group) {
    group.dataset.excerptHead = '0';
    group.dataset.excerptTail = '0';
    updateExcerpt(group);
  }

  function updateExcerpt(group) {
    const rows = excerptRows(group);
    const total = rows.length;
    let head = Math.min(Number(group.dataset.excerptHead || 0), total);
    let tail = Math.min(Number(group.dataset.excerptTail || 0), Math.max(0, total - head));
    const searchRevealAll = group.dataset.excerptSearchRevealAll === '1';

    if (head + tail > total) {
      tail = Math.max(0, total - head);
      group.dataset.excerptTail = String(tail);
    }

    const controls = group.querySelector('[data-role="excerpt-controls"]');
    const topControl = group.querySelector('[data-role="excerpt-expand-top"]');
    const bottomControl = group.querySelector('[data-role="excerpt-expand-bottom"]');

    rows.forEach((row, index) => {
      let visible = false;
      let order = index;

      if (searchRevealAll || index < head) {
        visible = true;
        order = index;
      } else if (index >= total - tail) {
        visible = true;
        order = head + 1 + (index - (total - tail));
      }

      row.classList.toggle('is-visible', visible);
      row.style.order = String(order);
    });

    if (controls) {
      controls.style.order = String(head);
    }

    const remaining = searchRevealAll ? 0 : Math.max(0, total - head - tail);
    const nextTopChunk = Math.min(EXCERPT_CHUNK, remaining);
    const nextBottomChunk = Math.min(EXCERPT_CHUNK, remaining);

    controls?.classList.toggle('filter-hidden', remaining === 0);
    topControl?.classList.toggle('filter-hidden', nextTopChunk === 0);
    bottomControl?.classList.toggle('filter-hidden', nextBottomChunk === 0);
    controls?.querySelector('[data-role="excerpt-label"]')?.replaceChildren(
      document.createTextNode(`${remaining} unchanged lines`)
    );
  }

  function expandExcerpt(group, side) {
    const rows = excerptRows(group);
    if (side === 'top') {
      const head = Number(group.dataset.excerptHead || 0);
      group.dataset.excerptHead = String(Math.min(rows.length, head + EXCERPT_CHUNK));
    } else {
      const tail = Number(group.dataset.excerptTail || 0);
      group.dataset.excerptTail = String(Math.min(rows.length, tail + EXCERPT_CHUNK));
    }
    updateExcerpt(group);
  }

  document.querySelectorAll('[data-role="excerpt-expand-top"]').forEach((button) => {
    button.addEventListener('click', () => {
      const group = button.closest('.context-group');
      if (!group) return;
      expandExcerpt(group, 'top');
    });
  });

  document.querySelectorAll('[data-role="excerpt-expand-bottom"]').forEach((button) => {
    button.addEventListener('click', () => {
      const group = button.closest('.context-group');
      if (!group) return;
      expandExcerpt(group, 'bottom');
    });
  });

  document.querySelectorAll('.context-group').forEach((group) => {
    initializeExcerpt(group);
  });

  fileCards.forEach((card) => {
    const summary = card.querySelector('summary');
    summary?.addEventListener('click', (event) => {
      event.preventDefault();
      const nextOpen = !card.open;
      setActiveFile(card.dataset.fileId || '', { scroll: false });
      card.open = nextOpen;
      syncToolbar();
    });
  });

  treeFiles.forEach((item) => {
    item.addEventListener('click', () => {
      const fileId = item.dataset.fileTarget || '';
      setActiveFile(fileId, { scroll: true });
    });
  });

  const preferred = firstPreferredCard();
  setDiffZoom(DEFAULT_DIFF_ZOOM);
  initializeHighlightedCode();
  if (preferred) {
    setActiveFile(preferred.dataset.fileId || '', { scroll: false });
  }
  if (initialState) {
    restoreDiffState(initialState);
  } else {
    syncActiveFileFromScroll();
    syncToolbar();
  }
})();
"###
}

fn tree_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4.5" y="4.5" width="15" height="15" rx="2.5"></rect><path d="M12 8v8"></path><path d="M8 12h8"></path></svg>"#
}

fn chevron_up_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 14.5L12 8.5l6 6"></path></svg>"#
}

fn chevron_down_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9.5l6 6 6-6"></path></svg>"#
}

fn jump_up_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 18V7"></path><path d="M7.5 11.5L12 7l4.5 4.5"></path><path d="M6 20h12"></path></svg>"#
}

fn jump_down_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 6v11"></path><path d="M7.5 12.5L12 17l4.5-4.5"></path><path d="M6 4h12"></path></svg>"#
}

fn search_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="6.5"></circle><path d="M16 16l4 4"></path></svg>"#
}

fn collapse_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 8.5h12"></path><path d="M8.5 12h7"></path><path d="M10.5 15.5h3"></path></svg>"#
}

fn fullscreen_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 4.5H4.5V8"></path><path d="M16 4.5h3.5V8"></path><path d="M8 19.5H4.5V16"></path><path d="M16 19.5h3.5V16"></path></svg>"#
}

fn close_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10"></path><path d="M17 7L7 17"></path></svg>"#
}

fn folder_icon() -> &'static str {
    r#"<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M1.75 3.25A.75.75 0 0 1 2.5 2.5h2.1l.95 1h3.95a.75.75 0 0 1 .75.75v5a.75.75 0 0 1-.75.75h-7a.75.75 0 0 1-.75-.75z"></path></svg>"#
}

fn chevron_icon() -> &'static str {
    r#"<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3.25 4.5 6 7.25 8.75 4.5"></path></svg>"#
}

fn file_icon() -> &'static str {
    r#"<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 1.75h3.25L8.5 4v6.25a.75.75 0 0 1-.75.75H3a.75.75 0 0 1-.75-.75V2.5A.75.75 0 0 1 3 1.75Z"></path><path d="M6.25 1.75V4H8.5"></path></svg>"#
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
