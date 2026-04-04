import { preloadFile } from "@pierre/diffs/ssr";

const bootstrap = window.__NOT_TERMINAL_SEARCH_BOOTSTRAP__ || {};
const worktreePath = String(bootstrap.worktreePath || "");

const root = document.createElement("div");
root.className = "search-shell";
root.innerHTML = `
  <div class="search-toolbar">
    <div class="toolbar-summary">
      <div class="toolbar-counts">
        <strong data-role="counts-primary">0 matches</strong>
        <span data-role="counts-secondary">0 files</span>
      </div>
      <div class="toolbar-path" data-role="scope">${escapeHtml(worktreePath)}</div>
    </div>
    <label class="toolbar-search">
      <span class="toolbar-search-icon">${iconSearch()}</span>
      <input id="project-search-input" type="search" placeholder="Search the project" spellcheck="false" autocomplete="off" data-role="search-input">
    </label>
    <div class="toolbar-actions">
      <button class="toolbar-btn" type="button" data-action="prev-match" title="Previous match" aria-label="Previous match">${iconChevronUp()}</button>
      <button class="toolbar-btn" type="button" data-action="next-match" title="Next match" aria-label="Next match">${iconChevronDown()}</button>
      <button class="toolbar-btn" type="button" data-action="toggle-fullscreen" title="Toggle fullscreen" aria-label="Toggle fullscreen">${iconFullscreen()}</button>
      <button class="toolbar-btn toolbar-btn-close" type="button" data-action="close-search" title="Close search" aria-label="Close search">${iconClose()}</button>
    </div>
  </div>
  <div class="search-layout">
    <aside class="results-panel">
      <div class="panel-header">
        <div class="panel-title">Results</div>
        <div class="panel-meta" data-role="results-meta">Type to search the active worktree</div>
      </div>
      <div class="tree-scroll" data-role="tree-scroll">
        <div class="tree-spacer" data-role="tree-spacer">
          <div class="tree-layer" data-role="tree-layer"></div>
        </div>
      </div>
    </aside>
    <section class="preview-panel">
      <div class="panel-header preview-header">
        <div class="preview-heading">
          <div class="panel-title" data-role="preview-title">No file selected</div>
          <div class="panel-meta" data-role="preview-subtitle">Search results will preview here</div>
        </div>
        <div class="preview-meta" data-role="preview-status"></div>
      </div>
      <div class="preview-scroll" data-role="preview-scroll">
        <div class="preview-surface" data-role="preview-surface"></div>
      </div>
    </section>
  </div>
`;

document.body.textContent = "";
document.body.appendChild(root);
injectStyle();

const handler =
  window.webkit &&
  window.webkit.messageHandlers &&
  window.webkit.messageHandlers.notTerminalDiff;

const searchInput = root.querySelector("[data-role='search-input']");
const countsPrimary = root.querySelector("[data-role='counts-primary']");
const countsSecondary = root.querySelector("[data-role='counts-secondary']");
const resultsMeta = root.querySelector("[data-role='results-meta']");
const treeScroll = root.querySelector("[data-role='tree-scroll']");
const treeSpacer = root.querySelector("[data-role='tree-spacer']");
const treeLayer = root.querySelector("[data-role='tree-layer']");
const previewTitle = root.querySelector("[data-role='preview-title']");
const previewSubtitle = root.querySelector("[data-role='preview-subtitle']");
const previewStatus = root.querySelector("[data-role='preview-status']");
const previewScroll = root.querySelector("[data-role='preview-scroll']");
const previewSurface = root.querySelector("[data-role='preview-surface']");

const state = {
  loading: false,
  query: "",
  error: "",
  results: { files: [], total_files: 0, total_matches: 0, truncated: false },
  treeRows: [],
  expandedFolders: new Set(),
  selectedPath: null,
  preview: null,
  previewCache: new Map(),
  previewRenderCache: new Map(),
  previewExpandState: new Map(),
  flatMatches: [],
  activeGlobalMatchIndex: 0,
  queryDebounce: 0,
};

const TREE_ROW_HEIGHT = 28;
const TREE_OVERSCAN = 10;
const PREVIEW_ROW_HEIGHT = 20;
const PREVIEW_OVERSCAN = 80;
const SEARCH_CONTEXT_LINES = 3;
const SEARCH_EXPAND_STEP = 10;
let previewContainer = null;
let previewMode = "empty";
let treeRenderQueued = false;
let previewRenderQueued = false;
let previewRenderToken = 0;

function postAction(action) {
  if (!handler) return;
  handler.postMessage(typeof action === "string" ? action : JSON.stringify(action));
}

function queueTreeRender() {
  if (treeRenderQueued) return;
  treeRenderQueued = true;
  requestAnimationFrame(() => {
    treeRenderQueued = false;
    renderTree();
  });
}

function summarizeCounts() {
  const totalMatches = state.results.total_matches || 0;
  const totalFiles = state.results.total_files || 0;
  if (!state.query) {
    countsPrimary.textContent = `${totalFiles} ${totalFiles === 1 ? "file" : "files"}`;
    countsSecondary.textContent = worktreePath ? "project" : "";
    resultsMeta.textContent = totalFiles
      ? "Browse files in the active worktree"
      : "Loading project files";
    return;
  }

  countsPrimary.textContent = `${totalMatches} ${totalMatches === 1 ? "match" : "matches"}`;
  countsSecondary.textContent = `${totalFiles} ${totalFiles === 1 ? "file" : "files"}`;

  if (state.loading) {
    resultsMeta.textContent = `Searching for “${state.query}”…`;
    return;
  }

  if (state.error) {
    resultsMeta.textContent = state.error;
    return;
  }

  if (!totalMatches) {
    resultsMeta.textContent = `No matches for “${state.query}”`;
    return;
  }

  resultsMeta.textContent = state.results.truncated
    ? "Showing capped results"
    : `Search results for “${state.query}”`;
}

function buildTree(files) {
  const rootNode = {
    type: "folder",
    name: "",
    path: "",
    children: [],
    matchCount: 0,
  };

  function ensureFolder(parent, name, path) {
    let folder = parent.children.find(
      (child) => child.type === "folder" && child.name === name,
    );
    if (!folder) {
      folder = { type: "folder", name, path, children: [], matchCount: 0 };
      parent.children.push(folder);
    }
    return folder;
  }

  for (const file of files) {
    const segments = file.path.split("/").filter(Boolean);
    let cursor = rootNode;
    let folderPath = "";
    for (let index = 0; index < Math.max(0, segments.length - 1); index += 1) {
      folderPath = folderPath ? `${folderPath}/${segments[index]}` : segments[index];
      cursor = ensureFolder(cursor, segments[index], folderPath);
    }
    cursor.children.push({
      type: "file",
      path: file.path,
      name: segments[segments.length - 1] || file.path,
      matchCount: file.match_count,
    });
  }

  function finalize(node) {
    if (node.type === "file") {
      return node;
    }

    node.children = node.children
      .map(finalize)
      .sort((left, right) => {
        if (left.type !== right.type) {
          return left.type === "folder" ? -1 : 1;
        }
        return (left.name || left.path).localeCompare(right.name || right.path);
      });

    node.matchCount = 0;
    for (const child of node.children) {
      node.matchCount += child.matchCount || 0;
    }

    while (
      node.path &&
      node.children.length === 1 &&
      node.children[0].type === "folder"
    ) {
      const child = node.children[0];
      node.name = `${node.name}/${child.name}`;
      node.path = child.path;
      node.children = child.children;
      node.matchCount = child.matchCount;
    }

    return node;
  }

  return finalize(rootNode);
}

function flattenTree() {
  const rows = [];
  const tree = buildTree(state.results.files || []);

  function walk(node, depth) {
    if (node.type === "folder") {
      if (node.path) {
        rows.push({
          kind: "folder",
          path: node.path,
          label: node.name,
          depth,
          matchCount: node.matchCount,
        });
      }

      const expanded = !node.path || state.expandedFolders.has(node.path);
      if (!expanded) return;
      for (const child of node.children) {
        walk(child, node.path ? depth + 1 : depth);
      }
      return;
    }

    rows.push({
      kind: "file",
      path: node.path,
      label: node.name,
      depth,
      matchCount: node.matchCount,
    });
  }

  walk(tree, 0);
  state.treeRows = rows;
}

function renderTree() {
  flattenTree();
  const viewportHeight = treeScroll.clientHeight || 0;
  const scrollTop = treeScroll.scrollTop;
  const totalHeight = state.treeRows.length * TREE_ROW_HEIGHT;
  treeSpacer.style.height = `${totalHeight}px`;

  const start = Math.max(0, Math.floor(scrollTop / TREE_ROW_HEIGHT) - TREE_OVERSCAN);
  const end = Math.min(
    state.treeRows.length,
    Math.ceil((scrollTop + viewportHeight) / TREE_ROW_HEIGHT) + TREE_OVERSCAN,
  );

  let html = "";
  for (let index = start; index < end; index += 1) {
    const row = state.treeRows[index];
    const depth = row.depth;
    const top = index * TREE_ROW_HEIGHT;
    const isFolder = row.kind === "folder";
    const isExpanded = isFolder && state.expandedFolders.has(row.path);
    const isSelected = !isFolder && row.path === state.selectedPath;
    const indent = depth * 10;
    const guides = Array.from({ length: depth }, (_, guideIndex) => {
      const left = 18 + guideIndex * 10;
      return `<span class="tree-guide" style="left:${left}px"></span>`;
    }).join("");

    html += `
      <div
        class="tree-row${isSelected ? " is-selected" : ""}${isFolder ? " is-folder" : " is-file"}"
        data-kind="${row.kind}"
        data-path="${escapeHtml(row.path)}"
        style="top:${top}px"
      >
        ${guides}
        <div class="tree-row-grid" style="padding-left:${12 + indent}px">
          <span class="tree-caret${isExpanded ? " is-expanded" : ""}" data-role="caret">
            ${isFolder ? iconTreeChevron() : ""}
          </span>
          <span class="tree-icon">${isFolder ? iconFolder() : iconFile()}</span>
          <span class="tree-label">${escapeHtml(row.label)}</span>
          <span class="tree-count">${row.matchCount > 0 ? row.matchCount : ""}</span>
        </div>
      </div>
    `;
  }

  treeLayer.innerHTML = html;
}

function rebuildFlatMatches() {
  const matches = [];
  for (const file of state.results.files || []) {
    for (const match of file.matches || []) {
      matches.push({
        path: file.path,
        line: match.line,
        column: match.column,
        end_column: match.end_column,
      });
    }
  }
  state.flatMatches = matches;
  if (state.activeGlobalMatchIndex >= matches.length) {
    state.activeGlobalMatchIndex = 0;
  }
}

function getSelectedFileMatches() {
  if (!state.selectedPath) return [];
  const file = (state.results.files || []).find((entry) => entry.path === state.selectedPath);
  return file ? file.matches || [] : [];
}

function getActiveMatch() {
  return state.flatMatches[state.activeGlobalMatchIndex] || null;
}

function activateGlobalMatch(index, { loadPreview = true } = {}) {
  if (!state.flatMatches.length) return;
  state.activeGlobalMatchIndex = ((index % state.flatMatches.length) + state.flatMatches.length) % state.flatMatches.length;
  const activeMatch = getActiveMatch();
  if (!activeMatch) return;
  const selectedChanged = state.selectedPath !== activeMatch.path;
  state.selectedPath = activeMatch.path;
  renderTree();
  if (selectedChanged && loadPreview) {
    requestPreview(activeMatch.path);
    return;
  }
  renderPreview();
  requestAnimationFrame(scrollPreviewToActiveMatch);
}

function moveMatch(offset) {
  if (!state.flatMatches.length) return;
  activateGlobalMatch(state.activeGlobalMatchIndex + offset);
}

function requestPreview(path) {
  if (!path) return;
  previewScroll.scrollTop = 0;
  previewScroll.scrollLeft = 0;
  const cached = state.previewCache.get(path);
  if (cached) {
    state.preview = cached;
    renderPreview();
    requestAnimationFrame(scrollPreviewToActiveMatch);
    return;
  }
  state.preview = null;
  renderPreview();
  postAction({ type: "select-file", path });
}

function ensureSelection() {
  const files = state.results.files || [];
  const paths = new Set(files.map((file) => file.path));

  if (state.selectedPath && paths.has(state.selectedPath)) {
    return;
  }

  if (!state.flatMatches.length) {
    state.selectedPath = files.length ? files[0].path : null;
    state.activeGlobalMatchIndex = 0;
  } else {
    state.activeGlobalMatchIndex = 0;
    state.selectedPath = state.flatMatches[0].path;
  }

  renderTree();
  if (state.selectedPath) {
    requestPreview(state.selectedPath);
  } else {
    state.preview = null;
    renderPreview();
  }
}

function createFilePayload(path, contents, suffix) {
  return {
    name: path,
    contents,
    cacheKey: `${path}:${suffix}:${contents.length}`,
  };
}

function disposePreviewInstance() {
  previewMode = "empty";
  if (previewContainer) {
    if (previewContainer.shadowRoot) {
      previewContainer.shadowRoot.innerHTML = "";
    }
    previewContainer.remove();
    previewContainer = null;
  }
}

function ensurePreviewContainer() {
  if (previewContainer && previewContainer.isConnected) {
    return previewContainer;
  }
  previewContainer = document.createElement("div");
  previewContainer.className = "preview-diffs-host";
  previewContainer.attachShadow({ mode: "open" });
  previewSurface.innerHTML = "";
  previewSurface.appendChild(previewContainer);
  return previewContainer;
}

function getPreviewRoot() {
  return previewContainer ? previewContainer.shadowRoot : null;
}

function queuePreviewRender() {
  if (previewRenderQueued) return;
  previewRenderQueued = true;
  requestAnimationFrame(() => {
    previewRenderQueued = false;
    if (previewMode === "full-file") {
      renderFullFileViewport();
    }
  });
}

function previewOptions() {
  return {
    theme: { dark: "pierre-dark", light: "pierre-light" },
    themeType: "dark",
    overflow: "scroll",
    disableFileHeader: true,
    unsafeCSS: `
      :host { color-scheme: dark; display: block; background: transparent !important; }
      pre { background: transparent !important; margin: 0 !important; }
      [data-diffs-pre] { background: transparent !important; }
      [data-code] { overflow: visible !important; }
      [data-gutter] { position: static !important; }
    `,
  };
}

function getPreviewModelKey(preview) {
  return `${preview.path}:${preview.contents.length}:${preview.line_count}:${preview.contents.slice(0, 64)}`;
}

function getPreviewExpandState(path) {
  let stateForPath = state.previewExpandState.get(path);
  if (!stateForPath) {
    stateForPath = new Map();
    state.previewExpandState.set(path, stateForPath);
  }
  return stateForPath;
}

async function getOrBuildPreviewModel(preview, token) {
  const key = getPreviewModelKey(preview);
  const cached = state.previewRenderCache.get(key);
  if (cached) {
    return cached;
  }

  const options = previewOptions();
  const prerendered = await preloadFile({
    file: createFilePayload(preview.path, preview.contents, "file"),
    options,
  });
  if (token !== previewRenderToken || state.preview !== preview) {
    return null;
  }

  const model = parsePrerenderedFile(prerendered.prerenderedHTML, preview.line_count);
  state.previewRenderCache.set(key, model);
  return model;
}

function parsePrerenderedFile(prerenderedHTML, lineCount) {
  const template = document.createElement("template");
  template.innerHTML = prerenderedHTML;

  const chromeNodes = [];
  for (const child of Array.from(template.content.children)) {
    if (
      child instanceof SVGElement ||
      (child instanceof HTMLElement && child.tagName === "STYLE")
    ) {
      chromeNodes.push(child.outerHTML);
    }
  }

  const pre = template.content.querySelector("pre");
  const gutter = pre ? pre.querySelector("[data-gutter]") : null;
  const content = pre ? pre.querySelector("[data-content]") : null;
  const gutterRows = Array.from(gutter?.children || []);
  const contentRows = Array.from(content?.children || []);
  const rowCount = Math.min(gutterRows.length, contentRows.length);
  const rows = [];

  for (let index = 0; index < rowCount; index += 1) {
    const gutterRow = gutterRows[index];
    const contentRow = contentRows[index];
    if (!(gutterRow instanceof HTMLElement) || !(contentRow instanceof HTMLElement)) {
      continue;
    }

    const lineNumber = Number.parseInt(
      contentRow.dataset.line || gutterRow.dataset.columnNumber || "",
      10,
    );
    if (!Number.isFinite(lineNumber)) {
      continue;
    }

    rows.push({
      lineNumber,
      gutterHTML: gutterRow.outerHTML,
      contentHTML: contentRow.outerHTML,
      lineType: contentRow.dataset.lineType || "context",
      text: contentRow.textContent || "",
    });
  }

  return {
    chromeHTML: chromeNodes.join(""),
    rows,
    lineCount,
  };
}

function previewShellStyles() {
  return `
    :host {
      display: block;
      color-scheme: dark;
      font-family: var(--diffs-font-family, var(--diffs-font-fallback));
      font-size: var(--diffs-font-size, 13px);
      line-height: var(--diffs-line-height, 20px);
      color: var(--diffs-fg, #edf1f7);
    }
    .preview-render-root {
      min-height: 100%;
      min-width: max-content;
      padding: 0 0 24px;
    }
    .preview-lines {
      min-width: max-content;
    }
    .preview-row,
    .preview-gap,
    .preview-spacer {
      display: grid;
      grid-template-columns: max-content minmax(0, 1fr);
      min-width: max-content;
    }
    .preview-row [data-column-number],
    .preview-gap-gutter,
    .preview-spacer-gutter {
      min-width: calc(${String(Math.max(3, String(state.preview?.line_count || 1).length))}ch + 4ch);
      text-align: right;
      padding-right: 1ch;
      color: #8c94a5;
      border-right: 1px solid rgba(255,255,255,0.08);
      background: rgba(255,255,255,0.01);
    }
    .preview-row [data-line],
    .preview-gap-main,
    .preview-spacer-main {
      min-width: 0;
    }
    .preview-gap-gutter,
    .preview-gap-main {
      min-height: 28px;
      display: flex;
      align-items: center;
      background: var(--diffs-bg-separator, rgba(255,255,255,0.04));
      border-top: 1px solid rgba(255,255,255,0.03);
      border-bottom: 1px solid rgba(255,255,255,0.03);
    }
    .preview-gap-gutter {
      justify-content: center;
      gap: 2px;
      padding: 0 6px;
    }
    .preview-gap-main {
      justify-content: space-between;
      gap: 10px;
      padding: 0 12px;
      color: var(--diffs-fg-number, #aeb6c7);
      user-select: none;
    }
    .preview-gap-label {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-family: var(--diffs-header-font-family, var(--diffs-header-font-fallback));
    }
    .preview-gap-controls {
      display: inline-flex;
      gap: 6px;
      flex: 0 0 auto;
    }
    .preview-gap-btn {
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--diffs-fg-number, #9dc1ff);
      min-width: 24px;
      height: 24px;
      padding: 0 8px;
      font-family: var(--diffs-header-font-family, var(--diffs-header-font-fallback));
      font-size: 12px;
      line-height: 1;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      justify-content: center;
    }
    .preview-gap-btn:hover {
      background: rgba(255,255,255,0.06);
      color: var(--diffs-fg, #d6e4ff);
    }
    .preview-gap-btn svg {
      width: 12px;
      height: 12px;
      stroke: currentColor;
      fill: none;
      stroke-width: 1.8;
      stroke-linecap: round;
      stroke-linejoin: round;
    }
    .preview-spacer-gutter,
    .preview-spacer-main {
      background: transparent;
      border: 0;
    }
  `;
}

function setPreviewMarkup(markup) {
  const host = ensurePreviewContainer();
  host.shadowRoot.innerHTML = `
    <style>${previewShellStyles()}</style>
    ${markup}
  `;
}

function renderRowsMarkup(rows, activeLine) {
  return rows
    .map((row) => {
      const selectedAttr = row.lineNumber === activeLine ? ' data-selected-line=""' : "";
      return `
        <div class="preview-row" data-preview-line="${row.lineNumber}">
          ${row.gutterHTML.replace("<div ", `<div${selectedAttr} `)}
          ${row.contentHTML.replace("<div ", `<div${selectedAttr} `)}
        </div>
      `;
    })
    .join("");
}

function mergeRanges(ranges) {
  if (!ranges.length) return [];
  const sorted = ranges
    .map(([start, end]) => [start, end])
    .sort((left, right) => left[0] - right[0]);
  const merged = [sorted[0]];
  for (let index = 1; index < sorted.length; index += 1) {
    const current = sorted[index];
    const previous = merged[merged.length - 1];
    if (current[0] <= previous[1] + 1) {
      previous[1] = Math.max(previous[1], current[1]);
    } else {
      merged.push(current);
    }
  }
  return merged;
}

function renderExcerptPreview(model, preview) {
  const activeMatch = getActiveMatch();
  const activeLine = activeMatch && activeMatch.path === preview.path ? activeMatch.line : -1;
  const matches = getSelectedFileMatches();
  const baseRanges = mergeRanges(
    matches.map((match) => [
      Math.max(1, match.line - SEARCH_CONTEXT_LINES),
      Math.min(model.lineCount, match.line + SEARCH_CONTEXT_LINES),
    ]),
  );
  const expandState = getPreviewExpandState(preview.path);
  const rowsByLine = new Map(model.rows.map((row) => [row.lineNumber, row]));
  const parts = [];
  let cursor = 1;

  for (let index = 0; index < baseRanges.length; index += 1) {
    const [rangeStart, rangeEnd] = baseRanges[index];
    if (rangeStart > cursor) {
      parts.push(renderGapMarkup(preview.path, cursor, rangeStart - 1, expandState, rowsByLine, activeLine));
    }

    const visibleRows = [];
    for (let line = rangeStart; line <= rangeEnd; line += 1) {
      const row = rowsByLine.get(line);
      if (row) visibleRows.push(row);
    }
    parts.push(renderRowsMarkup(visibleRows, activeLine));
    cursor = rangeEnd + 1;
  }

  if (cursor <= model.lineCount) {
    parts.push(renderGapMarkup(preview.path, cursor, model.lineCount, expandState, rowsByLine, activeLine));
  }

  setPreviewMarkup(`
    ${model.chromeHTML}
    <div class="preview-render-root">
      <div class="preview-lines">${parts.join("")}</div>
    </div>
  `);
  previewMode = "excerpt";
}

function renderGapMarkup(path, startLine, endLine, expandState, rowsByLine, activeLine) {
  if (startLine > endLine) return "";
  const key = `${startLine}:${endLine}`;
  const expanded = expandState.get(key) || { above: 0, below: 0 };
  const topEnd = Math.min(endLine, startLine + expanded.above - 1);
  const bottomStart = Math.max(startLine, endLine - expanded.below + 1);
  const hiddenStart = Math.max(startLine + expanded.above, startLine);
  const hiddenEnd = Math.min(endLine - expanded.below, endLine);
  const parts = [];

  if (expanded.above > 0) {
    const topRows = [];
    for (let line = startLine; line <= topEnd; line += 1) {
      const row = rowsByLine.get(line);
      if (row) topRows.push(row);
    }
    parts.push(renderRowsMarkup(topRows, activeLine));
  }

  const remaining = Math.max(0, hiddenEnd - hiddenStart + 1);
  if (remaining > 0) {
    parts.push(`
      <div class="preview-gap" data-gap-key="${key}">
        <div class="preview-gap-gutter">
          <button class="preview-gap-btn" type="button" data-gap-action="expand-above" data-gap-key="${key}" title="Show ${SEARCH_EXPAND_STEP} more above">${iconChevronUp()}</button>
          <button class="preview-gap-btn" type="button" data-gap-action="expand-below" data-gap-key="${key}" title="Show ${SEARCH_EXPAND_STEP} more below">${iconChevronDown()}</button>
        </div>
        <div class="preview-gap-main">
          <span class="preview-gap-label">${remaining} hidden ${remaining === 1 ? "line" : "lines"}</span>
          <span class="preview-gap-controls">
            <button class="preview-gap-btn" type="button" data-gap-action="expand-above" data-gap-key="${key}">+${SEARCH_EXPAND_STEP}</button>
            <button class="preview-gap-btn" type="button" data-gap-action="expand-below" data-gap-key="${key}">+${SEARCH_EXPAND_STEP}</button>
          </span>
        </div>
      </div>
    `);
  }

  if (expanded.below > 0) {
    const bottomRows = [];
    for (let line = bottomStart; line <= endLine; line += 1) {
      const row = rowsByLine.get(line);
      if (row) bottomRows.push(row);
    }
    parts.push(renderRowsMarkup(bottomRows, activeLine));
  }

  return parts.join("");
}

function renderFullFileViewport() {
  if (!state.preview) return;
  const key = getPreviewModelKey(state.preview);
  const model = state.previewRenderCache.get(key);
  if (!model) return;

  const activeMatch = getActiveMatch();
  const activeLine = activeMatch && activeMatch.path === state.preview.path ? activeMatch.line : -1;
  const viewportHeight = previewScroll.clientHeight || 0;
  const scrollTop = previewScroll.scrollTop;
  const startLine = Math.max(1, Math.floor(scrollTop / PREVIEW_ROW_HEIGHT) - PREVIEW_OVERSCAN);
  const endLine = Math.min(
    model.lineCount,
    Math.ceil((scrollTop + viewportHeight) / PREVIEW_ROW_HEIGHT) + PREVIEW_OVERSCAN,
  );
  const topSpacer = Math.max(0, (startLine - 1) * PREVIEW_ROW_HEIGHT);
  const bottomSpacer = Math.max(0, (model.lineCount - endLine) * PREVIEW_ROW_HEIGHT);
  const rowsByLine = new Map(model.rows.map((row) => [row.lineNumber, row]));
  const visibleRows = [];
  for (let line = startLine; line <= endLine; line += 1) {
    const row = rowsByLine.get(line);
    if (row) visibleRows.push(row);
  }

  setPreviewMarkup(`
    ${model.chromeHTML}
    <div class="preview-render-root">
      <div class="preview-lines">
        <div class="preview-spacer">
          <div class="preview-spacer-gutter" style="height:${topSpacer}px"></div>
          <div class="preview-spacer-main" style="height:${topSpacer}px"></div>
        </div>
        ${renderRowsMarkup(visibleRows, activeLine)}
        <div class="preview-spacer">
          <div class="preview-spacer-gutter" style="height:${bottomSpacer}px"></div>
          <div class="preview-spacer-main" style="height:${bottomSpacer}px"></div>
        </div>
      </div>
    </div>
  `);
  previewMode = "full-file";
}

function applyActiveLineSelection() {
  const activeMatch = getActiveMatch();
  const previewRoot = getPreviewRoot();
  if (!previewRoot) {
    return;
  }

  for (const node of previewRoot.querySelectorAll("[data-selected-line]")) {
    node.removeAttribute("data-selected-line");
  }

  if (!activeMatch || activeMatch.path !== state.selectedPath) {
    return;
  }

  for (const node of previewRoot.querySelectorAll(`[data-line="${activeMatch.line}"], [data-column-number="${activeMatch.line}"]`)) {
    node.setAttribute("data-selected-line", "");
  }
}

function scrollPreviewToActiveMatch() {
  const activeMatch = getActiveMatch();
  const previewRoot = getPreviewRoot();
  if (!previewRoot || !activeMatch || activeMatch.path !== state.selectedPath) {
    return;
  }

  const row = previewRoot.querySelector(`[data-preview-line="${activeMatch.line}"], [data-line="${activeMatch.line}"]`);
  if (!row) {
    return;
  }

  const rowBox = row.getBoundingClientRect();
  const scrollBox = previewScroll.getBoundingClientRect();
  const topDelta = rowBox.top - scrollBox.top;
  const desiredTop = previewScroll.clientHeight * 0.28;
  previewScroll.scrollTop = Math.max(0, previewScroll.scrollTop + topDelta - desiredTop);
}

function activateNearestMatchForLine(lineNumber) {
  if (!state.selectedPath) return;
  const fileMatches = getSelectedFileMatches();
  if (!fileMatches.length) return;

  let bestIndex = 0;
  let bestDistance = Infinity;
  for (let index = 0; index < fileMatches.length; index += 1) {
    const distance = Math.abs(fileMatches[index].line - lineNumber);
    if (distance < bestDistance) {
      bestDistance = distance;
      bestIndex = index;
    }
    if (distance === 0) break;
  }

  const target = fileMatches[bestIndex];
  const globalIndex = state.flatMatches.findIndex(
    (entry) => entry.path === state.selectedPath && entry.line === target.line && entry.column === target.column,
  );
  if (globalIndex >= 0) {
    activateGlobalMatch(globalIndex, { loadPreview: false });
  }
}

async function renderPreview() {
  if (state.loading && !state.preview) {
    previewRenderToken += 1;
    disposePreviewInstance();
    previewTitle.textContent = "Searching…";
    previewSubtitle.textContent = state.query;
    previewStatus.textContent = "";
    previewSurface.innerHTML = `<div class="preview-empty">Finding matches…</div>`;
    return;
  }

  if (!state.preview) {
    previewRenderToken += 1;
    disposePreviewInstance();
    previewTitle.textContent = state.selectedPath || "No file selected";
    previewSubtitle.textContent = state.query ? "Select a file to preview" : "Choose a file from the tree";
    previewStatus.textContent = "";
    previewSurface.innerHTML = `<div class="preview-empty">${state.query ? "No preview available" : "Select a file to start browsing"}</div>`;
    return;
  }

  const activeFileMatches = getSelectedFileMatches();
  const activeMatch = getActiveMatch();
  const preview = state.preview;
  const token = ++previewRenderToken;

  previewTitle.textContent = preview.path;
  previewSubtitle.textContent = state.query
    ? `Search excerpts in ${preview.line_count} ${preview.line_count === 1 ? "line" : "lines"}`
    : `${preview.line_count} ${preview.line_count === 1 ? "line" : "lines"}`;
  previewStatus.textContent = activeFileMatches.length
    ? `${activeMatch && activeMatch.path === preview.path ? activeFileMatches.findIndex((entry) => entry.line === activeMatch.line && entry.column === activeMatch.column) + 1 : 1} / ${activeFileMatches.length}`
    : "";

  const model = await getOrBuildPreviewModel(preview, token);
  if (!model || token !== previewRenderToken) {
    return;
  }

  if (state.query) {
    renderExcerptPreview(model, preview);
  } else {
    renderFullFileViewport();
  }
  applyActiveLineSelection();
}

function setResults(payload) {
  state.loading = false;
  state.error = payload && payload.error ? payload.error : "";
  state.results = payload || { files: [], total_files: 0, total_matches: 0, truncated: false };

  const expanded = new Set();
  for (const file of state.results.files || []) {
    const segments = file.path.split("/").filter(Boolean);
    let path = "";
    for (let index = 0; index < Math.max(0, segments.length - 1); index += 1) {
      path = path ? `${path}/${segments[index]}` : segments[index];
      expanded.add(path);
    }
  }
  state.expandedFolders = expanded;

  summarizeCounts();
  rebuildFlatMatches();
  queueTreeRender();
  ensureSelection();
}

function setPreview(payload) {
  if (!payload) {
    state.preview = null;
    renderPreview();
    return;
  }

  state.previewCache.set(payload.path, payload);
  if (payload.path === state.selectedPath) {
    state.preview = payload;
    renderPreview();
    requestAnimationFrame(scrollPreviewToActiveMatch);
  }
}

function setLoading(payload) {
  state.loading = true;
  state.error = "";
  if (payload && typeof payload.query === "string") {
    state.query = payload.query;
  }
  state.previewRenderCache.clear();
  state.previewExpandState.clear();
  summarizeCounts();
  renderPreview();
}

function handleTreeClick(event) {
  const row = event.target.closest(".tree-row");
  if (!row) return;
  const kind = row.dataset.kind;
  const path = row.dataset.path;
  if (!path) return;

  if (kind === "folder") {
    if (state.expandedFolders.has(path)) {
      state.expandedFolders.delete(path);
    } else {
      state.expandedFolders.add(path);
    }
    queueTreeRender();
    return;
  }

  state.selectedPath = path;
  const fileMatches = getSelectedFileMatches();
  if (fileMatches.length) {
    const globalIndex = state.flatMatches.findIndex(
      (entry) => entry.path === path && entry.line === fileMatches[0].line && entry.column === fileMatches[0].column,
    );
    if (globalIndex >= 0) {
      state.activeGlobalMatchIndex = globalIndex;
    }
  }
  queueTreeRender();
  requestPreview(path);
}

function handleTreeDoubleClick(event) {
  const row = event.target.closest(".tree-row");
  if (!row || row.dataset.kind !== "file") return;
  const path = row.dataset.path;
  if (!path) return;
  const file = (state.results.files || []).find((entry) => entry.path === path);
  const firstMatch = file && file.matches ? file.matches[0] : null;
  if (!firstMatch) return;
  postAction({
    type: "open-result",
    path,
    line: firstMatch.line,
    column: firstMatch.column,
  });
}

function handleSearchInput() {
  const value = searchInput.value;
  state.query = value;
  state.previewCache.clear();
  state.previewRenderCache.clear();
  state.previewExpandState.clear();
  clearTimeout(state.queryDebounce);
  state.queryDebounce = window.setTimeout(() => {
    postAction({ type: "query-changed", query: value });
    setLoading({ query: value });
  }, 75);
}

function getPreviewLineElement(event) {
  for (const item of event.composedPath()) {
    if (item instanceof HTMLElement && item.hasAttribute("data-line")) {
      return item;
    }
  }
  return null;
}

function handlePreviewClick(event) {
  const gapAction = event.composedPath().find(
    (item) => item instanceof HTMLElement && item.hasAttribute("data-gap-action"),
  );
  if (gapAction && state.selectedPath) {
    const key = gapAction.dataset.gapKey;
    if (key) {
      const expandState = getPreviewExpandState(state.selectedPath);
      const previous = expandState.get(key) || { above: 0, below: 0 };
      if (gapAction.dataset.gapAction === "expand-above") {
        expandState.set(key, { ...previous, above: previous.above + SEARCH_EXPAND_STEP });
      } else if (gapAction.dataset.gapAction === "expand-below") {
        expandState.set(key, { ...previous, below: previous.below + SEARCH_EXPAND_STEP });
      }
      renderPreview();
    }
    return;
  }

  const line = getPreviewLineElement(event);
  if (!line) return;

  const lineNumber = Number.parseInt(line.dataset.line || "", 10);
  if (!Number.isFinite(lineNumber)) return;
  activateNearestMatchForLine(lineNumber);
}

function handlePreviewDoubleClick(event) {
  const line = getPreviewLineElement(event);
  if (!line || !state.selectedPath) return;

  const lineNumber = Number.parseInt(line.dataset.line || "", 10);
  if (!Number.isFinite(lineNumber)) return;
  const match = getSelectedFileMatches().find((entry) => entry.line === lineNumber);
  if (!match) return;
  postAction({
    type: "open-result",
    path: state.selectedPath,
    line: match.line,
    column: match.column,
  });
}

function bindTextInputFocus(input) {
  input.addEventListener("focus", () => postAction("enable-text-input"));
  input.addEventListener("blur", () => {
    window.setTimeout(() => {
      if (document.activeElement !== input) {
        postAction("disable-text-input");
      }
    }, 0);
  });
}

treeScroll.addEventListener("scroll", queueTreeRender);
treeLayer.addEventListener("click", handleTreeClick);
treeLayer.addEventListener("dblclick", handleTreeDoubleClick);
previewScroll.addEventListener("scroll", queuePreviewRender);
previewSurface.addEventListener("click", handlePreviewClick);
previewSurface.addEventListener("dblclick", handlePreviewDoubleClick);
searchInput.addEventListener("input", handleSearchInput);
bindTextInputFocus(searchInput);
window.addEventListener("resize", () => {
  queueTreeRender();
  requestAnimationFrame(scrollPreviewToActiveMatch);
});

root.querySelector("[data-action='prev-match']").addEventListener("click", () => moveMatch(-1));
root.querySelector("[data-action='next-match']").addEventListener("click", () => moveMatch(1));
root.querySelector("[data-action='toggle-fullscreen']").addEventListener("click", () =>
  postAction("toggle-split-zoom"),
);
root.querySelector("[data-action='close-search']").addEventListener("click", () =>
  postAction("toggle-project-search-view"),
);

window.__NOT_TERMINAL_SEARCH_SET_RESULTS__ = setResults;
window.__NOT_TERMINAL_SEARCH_SET_PREVIEW__ = setPreview;
window.__NOT_TERMINAL_SEARCH_SET_LOADING__ = setLoading;

summarizeCounts();
queueTreeRender();
renderPreview();

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function injectStyle() {
  const style = document.createElement("style");
  style.textContent = `
    :root {
      color-scheme: dark;
      --bg: #0f1013;
      --panel: #121418;
      --panel-alt: #151820;
      --border: rgba(114, 124, 146, 0.18);
      --border-soft: rgba(114, 124, 146, 0.1);
      --text: #edf1f7;
      --muted: #8c94a5;
      --muted-strong: #aeb6c7;
      --accent: #7bb3ff;
      --accent-soft: rgba(123, 179, 255, 0.14);
    }
    * { box-sizing: border-box; }
    html, body {
      margin: 0;
      width: 100%;
      height: 100%;
      overflow: hidden;
      background: var(--bg);
      color: var(--text);
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 13px;
    }
    button, input { font: inherit; }
    .search-shell {
      display: grid;
      grid-template-rows: auto 1fr;
      width: 100%;
      height: 100%;
      background: linear-gradient(180deg, rgba(255,255,255,0.02), transparent 36%), var(--bg);
    }
    .search-toolbar {
      display: grid;
      grid-template-columns: minmax(0, 1fr) minmax(260px, 520px) auto;
      gap: 12px;
      align-items: center;
      padding: 10px 14px;
      border-bottom: 1px solid var(--border);
      background: rgba(12, 13, 16, 0.96);
      backdrop-filter: blur(18px);
    }
    .toolbar-summary {
      min-width: 0;
      display: grid;
      gap: 2px;
    }
    .toolbar-counts {
      display: flex;
      align-items: baseline;
      gap: 10px;
      white-space: nowrap;
    }
    .toolbar-counts strong {
      font-size: 13px;
      color: var(--text);
      font-weight: 700;
    }
    .toolbar-counts span {
      color: var(--muted);
    }
    .toolbar-path {
      min-width: 0;
      color: var(--muted);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .toolbar-search {
      display: flex;
      align-items: center;
      gap: 9px;
      min-width: 0;
      height: 32px;
      padding: 0 12px;
      border: 1px solid var(--border-soft);
      border-radius: 9px;
      background: rgba(255,255,255,0.02);
    }
    .toolbar-search:focus-within {
      border-color: rgba(123, 179, 255, 0.35);
      background: rgba(123, 179, 255, 0.07);
    }
    .toolbar-search input {
      width: 100%;
      min-width: 0;
      border: 0;
      outline: none;
      background: transparent;
      color: var(--text);
      padding: 0;
    }
    .toolbar-search input::placeholder {
      color: var(--muted);
    }
    .toolbar-search-icon,
    .toolbar-btn svg,
    .tree-caret svg,
    .tree-icon svg {
      width: 14px;
      height: 14px;
      stroke: currentColor;
      fill: none;
      stroke-width: 1.8;
      stroke-linecap: round;
      stroke-linejoin: round;
    }
    .toolbar-search-icon {
      color: var(--muted);
      display: inline-flex;
      align-items: center;
      justify-content: center;
      flex: 0 0 auto;
    }
    .toolbar-actions {
      display: flex;
      align-items: center;
      gap: 2px;
    }
    .toolbar-btn {
      width: 28px;
      height: 28px;
      border: 0;
      border-radius: 7px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      color: var(--muted);
      background: transparent;
      cursor: pointer;
    }
    .toolbar-btn:hover {
      color: var(--text);
      background: rgba(255,255,255,0.05);
    }
    .toolbar-btn-close:hover {
      color: #ffb6c0;
      background: rgba(255, 112, 138, 0.08);
    }
    .search-layout {
      min-height: 0;
      display: grid;
      grid-template-columns: 300px minmax(0, 1fr);
      grid-template-rows: 1fr;
    }
    .results-panel {
      min-width: 0;
      min-height: 0;
      display: grid;
      grid-template-rows: auto 1fr;
      border-right: 1px solid var(--border);
      background: linear-gradient(180deg, rgba(255,255,255,0.015), transparent 45%);
    }
    .preview-panel {
      min-width: 0;
      min-height: 0;
      display: grid;
      grid-template-rows: auto 1fr;
    }
    .panel-header {
      padding: 12px 14px 10px;
      border-bottom: 1px solid var(--border);
    }
    .panel-title {
      color: var(--text);
      font-size: 12px;
      font-weight: 700;
      letter-spacing: 0.02em;
    }
    .panel-meta {
      margin-top: 3px;
      color: var(--muted);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .preview-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
    }
    .preview-heading {
      min-width: 0;
    }
    .preview-meta {
      color: var(--muted);
      white-space: nowrap;
    }
    .tree-scroll,
    .preview-scroll {
      min-height: 0;
      overflow: auto;
    }
    .tree-spacer {
      position: relative;
      min-height: 100%;
    }
    .tree-layer {
      position: absolute;
      inset: 0;
    }
    .tree-row {
      position: absolute;
      left: 0;
      right: 0;
      height: ${TREE_ROW_HEIGHT}px;
    }
    .tree-row-grid {
      position: relative;
      display: grid;
      grid-template-columns: 14px 14px minmax(0, 1fr) auto;
      align-items: center;
      gap: 7px;
      height: 100%;
      padding-right: 12px;
      border-left: 2px solid transparent;
    }
    .tree-row.is-selected .tree-row-grid {
      background: rgba(255,255,255,0.035);
      border-left-color: var(--accent);
    }
    .tree-row.is-file { cursor: pointer; }
    .tree-row.is-file:hover .tree-row-grid,
    .tree-row.is-folder:hover .tree-row-grid {
      background: rgba(255,255,255,0.025);
    }
    .tree-guide {
      position: absolute;
      top: 6px;
      bottom: 6px;
      width: 1px;
      background: rgba(255,255,255,0.06);
    }
    .tree-caret,
    .tree-icon {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      color: var(--muted-strong);
    }
    .tree-caret {
      width: 14px;
      height: 14px;
      cursor: pointer;
    }
    .tree-caret.is-expanded svg {
      transform: rotate(90deg);
    }
    .tree-label {
      min-width: 0;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      color: var(--text);
      font-weight: 600;
    }
    .tree-count {
      color: var(--muted);
    }
    .preview-surface {
      min-height: 100%;
      padding: 12px 14px 18px;
    }
    .preview-diffs-host {
      min-height: 100%;
      display: block;
    }
    .preview-empty {
      min-height: 100%;
      display: grid;
      place-items: center;
      color: var(--muted);
    }
  `;
  document.head.appendChild(style);
}

function iconSearch() {
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.5"></circle><path d="M10.5 10.5L14 14"></path></svg>`;
}

function iconChevronUp() {
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 10.5L8 6.5l3.5 4"></path></svg>`;
}

function iconChevronDown() {
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 5.5L8 9.5l3.5-4"></path></svg>`;
}

function iconTreeChevron() {
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3.5L10 8L6 12.5"></path></svg>`;
}

function iconFolder() {
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.75 4.75h4l1.4 1.6H14a.5.5 0 0 1 .5.5v5.15a1 1 0 0 1-1 1H2.5a1 1 0 0 1-1-1v-6.75a.5.5 0 0 1 .5-.5z"></path></svg>`;
}

function iconFile() {
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 1.75h5l3 3v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1z"></path><path d="M9 1.75v3h3"></path></svg>`;
}

function iconFullscreen() {
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 2.5H2.5V6"></path><path d="M10 2.5h3.5V6"></path><path d="M6 13.5H2.5V10"></path><path d="M10 13.5h3.5V10"></path></svg>`;
}

function iconClose() {
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8"></path><path d="M12 4l-8 8"></path></svg>`;
}
