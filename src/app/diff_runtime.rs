use crate::app::git_diff;
use crate::webview::WebView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffPaneAction {
    ToggleSplitZoom,
    ToggleDiffView,
    FontIncrease,
    FontDecrease,
    FontReset,
}

pub(crate) struct DiffPaneRuntime {
    pub(crate) id: String,
    pub(crate) webview: WebView,
    pub(crate) worktree_path: String,
    pub(crate) watch_paths: Vec<String>,
    pub(crate) last_html: String,
    pub(crate) last_frame: Option<(f64, f64, f64, f64)>,
    pub(crate) last_hidden: Option<bool>,
}

const DIFF_STATE_CAPTURE_SCRIPT: &str = r#"window.__NOT_TERMINAL_DIFF_CAPTURE_STATE__ ? window.__NOT_TERMINAL_DIFF_CAPTURE_STATE__() : "";"#;
const DIFF_ZOOM_INCREASE_SCRIPT: &str = r#"window.__NOT_TERMINAL_DIFF_ADJUST_ZOOM__ ? window.__NOT_TERMINAL_DIFF_ADJUST_ZOOM__(1) : "";"#;
const DIFF_ZOOM_DECREASE_SCRIPT: &str = r#"window.__NOT_TERMINAL_DIFF_ADJUST_ZOOM__ ? window.__NOT_TERMINAL_DIFF_ADJUST_ZOOM__(-1) : "";"#;
const DIFF_ZOOM_RESET_SCRIPT: &str =
    r#"window.__NOT_TERMINAL_DIFF_RESET_ZOOM__ ? window.__NOT_TERMINAL_DIFF_RESET_ZOOM__() : "";"#;

impl DiffPaneRuntime {
    pub(crate) fn new(
        id: String,
        webview: WebView,
        worktree_path: String,
        watch_paths: Vec<String>,
    ) -> Self {
        let html = git_diff::render_loading_html(&worktree_path);
        webview.load_html(&html);

        Self {
            id,
            webview,
            worktree_path,
            watch_paths,
            last_html: html,
            last_frame: None,
            last_hidden: None,
        }
    }

    pub(crate) fn update_html(&mut self, html: &str) -> bool {
        if self.last_html == html {
            return false;
        }

        let preserved_state = self.capture_state();
        let next_html = preserved_state
            .as_deref()
            .map(|state| git_diff::inject_preserved_state(html, state))
            .unwrap_or_else(|| html.to_string());

        self.webview.load_html(&next_html);
        self.last_html.clear();
        self.last_html.push_str(html);
        true
    }

    fn capture_state(&self) -> Option<String> {
        self.webview
            .evaluate_javascript(DIFF_STATE_CAPTURE_SCRIPT)
            .filter(|state| !state.trim().is_empty() && state.trim() != "null")
    }

    pub(crate) fn take_action(&self) -> Option<DiffPaneAction> {
        let action = self.webview.take_action()?;
        match action.as_str() {
            "toggle-split-zoom" => Some(DiffPaneAction::ToggleSplitZoom),
            "toggle-diff-view" => Some(DiffPaneAction::ToggleDiffView),
            "diff-font-increase" => Some(DiffPaneAction::FontIncrease),
            "diff-font-decrease" => Some(DiffPaneAction::FontDecrease),
            "diff-font-reset" => Some(DiffPaneAction::FontReset),
            _ => None,
        }
    }

    pub(crate) fn increase_font_size(&self) {
        let _ = self.webview.evaluate_javascript(DIFF_ZOOM_INCREASE_SCRIPT);
    }

    pub(crate) fn decrease_font_size(&self) {
        let _ = self.webview.evaluate_javascript(DIFF_ZOOM_DECREASE_SCRIPT);
    }

    pub(crate) fn reset_font_size(&self) {
        let _ = self.webview.evaluate_javascript(DIFF_ZOOM_RESET_SCRIPT);
    }
}
