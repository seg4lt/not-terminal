use crate::app::git_diff;
use crate::webview::WebView;

pub(crate) struct DiffPaneRuntime {
    pub(crate) id: String,
    pub(crate) webview: WebView,
    pub(crate) worktree_path: String,
    pub(crate) last_frame: Option<(f64, f64, f64, f64)>,
    pub(crate) last_hidden: Option<bool>,
}

impl DiffPaneRuntime {
    pub(crate) fn new(id: String, webview: WebView, worktree_path: String) -> Self {
        let html = git_diff::render_loading_html(&worktree_path);
        webview.load_html(&html);

        Self {
            id,
            webview,
            worktree_path,
            last_frame: None,
            last_hidden: None,
        }
    }
}
