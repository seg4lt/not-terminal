use crate::app::project_search::{
    ProjectSearchFile, ProjectSearchPreview, ProjectSearchRequest, ProjectSearchResponse,
};
use crate::app::project_search_view;
use crate::webview::WebView;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SearchPaneAction {
    ToggleSplitZoom,
    ToggleProjectSearchView,
    QueryChanged(ProjectSearchRequest),
    SelectFile(String),
    OpenResult {
        path: String,
        line: usize,
        column: usize,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum SearchPaneActionEnvelope {
    ToggleSplitZoom,
    ToggleProjectSearchView,
    QueryChanged {
        query: String,
        include: Option<String>,
        exclude: Option<String>,
        include_gitignored: Option<bool>,
        case_sensitive: Option<bool>,
    },
    SelectFile {
        path: String,
    },
    OpenResult {
        path: String,
        line: usize,
        column: usize,
    },
}

pub(crate) struct SearchPaneRuntime {
    pub(crate) id: String,
    pub(crate) webview: WebView,
    pub(crate) worktree_path: String,
    pub(crate) last_frame: Option<(f64, f64, f64, f64)>,
    pub(crate) last_hidden: Option<bool>,
    pub(crate) current_request: ProjectSearchRequest,
    pub(crate) request_id: u64,
    pub(crate) preview_request_id: u64,
    pub(crate) selected_preview_path: Option<String>,
    pub(crate) files: Vec<ProjectSearchFile>,
}

impl SearchPaneRuntime {
    pub(crate) fn new(id: String, webview: WebView, worktree_path: String) -> Self {
        let html = project_search_view::render_shell_html(&worktree_path);
        webview.load_html(&html);

        Self {
            id,
            webview,
            worktree_path,
            last_frame: None,
            last_hidden: None,
            current_request: ProjectSearchRequest::default(),
            request_id: 0,
            preview_request_id: 0,
            selected_preview_path: None,
            files: Vec::new(),
        }
    }

    pub(crate) fn take_action(&self) -> Option<SearchPaneAction> {
        let action = self.webview.take_action()?;
        if action == "toggle-split-zoom" {
            return Some(SearchPaneAction::ToggleSplitZoom);
        }
        if action == "toggle-project-search-view" {
            return Some(SearchPaneAction::ToggleProjectSearchView);
        }

        let envelope = serde_json::from_str::<SearchPaneActionEnvelope>(&action).ok()?;
        match envelope {
            SearchPaneActionEnvelope::ToggleSplitZoom => Some(SearchPaneAction::ToggleSplitZoom),
            SearchPaneActionEnvelope::ToggleProjectSearchView => {
                Some(SearchPaneAction::ToggleProjectSearchView)
            }
            SearchPaneActionEnvelope::QueryChanged {
                query,
                include,
                exclude,
                include_gitignored,
                case_sensitive,
            } => Some(SearchPaneAction::QueryChanged(ProjectSearchRequest {
                query,
                options: crate::app::project_search::ProjectSearchOptions {
                    include: include.unwrap_or_default(),
                    exclude: exclude.unwrap_or_default(),
                    include_gitignored: include_gitignored.unwrap_or(false),
                    case_sensitive: case_sensitive.unwrap_or(true),
                },
            })),
            SearchPaneActionEnvelope::SelectFile { path } => {
                Some(SearchPaneAction::SelectFile(path))
            }
            SearchPaneActionEnvelope::OpenResult { path, line, column } => {
                Some(SearchPaneAction::OpenResult { path, line, column })
            }
        }
    }

    pub(crate) fn begin_query(&mut self, request: ProjectSearchRequest) -> u64 {
        self.current_request = request;
        self.request_id = self.request_id.wrapping_add(1);
        self.preview_request_id = self.preview_request_id.wrapping_add(1);
        self.selected_preview_path = None;
        self.files.clear();
        self.request_id
    }

    pub(crate) fn begin_preview(
        &mut self,
        path: String,
    ) -> Option<(u64, Vec<crate::app::project_search::ProjectSearchMatch>)> {
        let matches = self
            .files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.matches.clone())?;
        self.preview_request_id = self.preview_request_id.wrapping_add(1);
        self.selected_preview_path = Some(path);
        Some((self.preview_request_id, matches))
    }

    pub(crate) fn matches_query_response(
        &self,
        request_id: u64,
        request: &ProjectSearchRequest,
    ) -> bool {
        self.request_id == request_id && self.current_request == *request
    }

    pub(crate) fn matches_preview_response(&self, request_id: u64, path: &str) -> bool {
        self.preview_request_id == request_id && self.selected_preview_path.as_deref() == Some(path)
    }

    pub(crate) fn set_loading(&self, query: &str) {
        self.invoke_json_function(
            "window.__NOT_TERMINAL_SEARCH_SET_LOADING__",
            &serde_json::json!({
                "query": query,
            }),
        );
    }

    pub(crate) fn set_error(&mut self, query: &str, error: &str) {
        self.files.clear();
        let payload = serde_json::json!({
            "query": query,
            "total_files": 0,
            "total_matches": 0,
            "truncated": false,
            "files": [],
            "error": error,
        });
        self.invoke_json_function("window.__NOT_TERMINAL_SEARCH_SET_RESULTS__", &payload);
        self.clear_preview();
    }

    pub(crate) fn set_results(&mut self, response: &ProjectSearchResponse) {
        self.set_results_with_loading(response, false);
    }

    pub(crate) fn set_results_loading(&mut self, response: &ProjectSearchResponse) {
        self.set_results_with_loading(response, true);
    }

    fn set_results_with_loading(&mut self, response: &ProjectSearchResponse, loading: bool) {
        self.files = response.files.clone();
        let payload = serde_json::json!({
            "query": &response.query,
            "total_files": response.total_files,
            "total_matches": response.total_matches,
            "truncated": response.truncated,
            "files": &response.files,
            "loading": loading,
        });
        self.invoke_json_function("window.__NOT_TERMINAL_SEARCH_SET_RESULTS__", &payload);
        if response.files.is_empty() {
            self.clear_preview();
        }
    }
    pub(crate) fn set_preview(&self, preview: &ProjectSearchPreview) {
        self.invoke_json_function("window.__NOT_TERMINAL_SEARCH_SET_PREVIEW__", preview);
    }

    pub(crate) fn clear_preview(&self) {
        self.invoke_json_function(
            "window.__NOT_TERMINAL_SEARCH_SET_PREVIEW__",
            &serde_json::json!(null),
        );
    }

    fn invoke_json_function<T: serde::Serialize>(&self, function_name: &str, payload: &T) {
        let Ok(json) = serde_json::to_string(payload) else {
            return;
        };
        let script = format!("{} && {}({});", function_name, function_name, json);
        let _ = self.webview.evaluate_javascript(&script);
    }
}
