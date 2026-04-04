mod diff_runtime;
mod git_branch;
mod git_diff;
mod git_watch;
mod git_worktrees;
mod model;
mod persistence;
mod project_search;
mod project_search_view;
mod runtime;
mod search_runtime;
mod shortcuts;
mod state;
mod update;
mod view;

use model::default_show_native_title_bar;

pub(crate) use state::App;
pub(crate) use update::update;
pub(crate) use view::view;

pub(crate) fn initial_show_native_title_bar() -> bool {
    persistence::load_state()
        .map(|state| state.ui.show_native_title_bar)
        .unwrap_or_else(|_| default_show_native_title_bar())
}
