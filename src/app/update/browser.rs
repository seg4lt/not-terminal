use crate::app::state::{App, Message};
use crate::webview::WebView;
use iced::Task;

pub(super) fn handle_add_browser(app: &mut App) -> Task<Message> {
    let browser_id = app.add_browser();
    // Create webview for the new browser
    if let Some(host_ns_view) = app.host_ns_view {
        if let Some(webview) = WebView::new_hosted(host_ns_view) {
            app.browser_webviews.insert(browser_id, webview);
        }
    }
    app.sync_runtime_views();
    Task::none()
}

pub(super) fn handle_remove_browser(app: &mut App, browser_id: String) -> Task<Message> {
    app.remove_browser(&browser_id);
    app.sync_runtime_views();
    app.save_task()
}

pub(super) fn handle_select_browser(app: &mut App, browser_id: String) -> Task<Message> {
    app.select_browser(&browser_id);
    // Ensure webview exists
    if !app.browser_webviews.contains_key(&browser_id)
        && let Some(host_ns_view) = app.host_ns_view
        && let Some(webview) = WebView::new_hosted(host_ns_view)
    {
        app.browser_webviews.insert(browser_id.clone(), webview);
    }
    // Load URL if this is the first time selecting
    if let Some(browser) = app.active_browser()
        && let Some(webview) = app.browser_webviews.get(&browser_id)
        && !browser.url.is_empty()
        && browser.url != "https://"
    {
        webview.load_url(&browser.url);
    }
    app.sync_runtime_views();
    app.save_task()
}

pub(super) fn handle_browser_url_changed(app: &mut App, value: String) -> Task<Message> {
    if let Some(browser_id) = app.active_browser_id()
        && let Some(b) = app
            .persisted
            .browsers
            .iter_mut()
            .find(|b| b.id == browser_id)
    {
        b.url = value;
    }
    Task::none()
}

pub(super) fn handle_browser_navigate(app: &mut App) -> Task<Message> {
    if let Some(browser) = app.active_browser() {
        let url = browser.url.trim().to_string();
        if !url.is_empty()
            && let Some(webview) = app.browser_webviews.get(&browser.id)
        {
            webview.load_url(&url);
        }
    }
    Task::none()
}

pub(super) fn handle_browser_back(app: &mut App) -> Task<Message> {
    if let Some(browser) = app.active_browser()
        && let Some(webview) = app.browser_webviews.get(&browser.id)
    {
        webview.go_back();
    }
    Task::none()
}

pub(super) fn handle_browser_forward(app: &mut App) -> Task<Message> {
    if let Some(browser) = app.active_browser()
        && let Some(webview) = app.browser_webviews.get(&browser.id)
    {
        webview.go_forward();
    }
    Task::none()
}

pub(super) fn handle_browser_reload(app: &mut App) -> Task<Message> {
    if let Some(browser) = app.active_browser()
        && let Some(webview) = app.browser_webviews.get(&browser.id)
    {
        webview.reload();
    }
    Task::none()
}

pub(super) fn handle_browser_devtools(app: &mut App) -> Task<Message> {
    if let Some(browser) = app.active_browser()
        && let Some(webview) = app.browser_webviews.get(&browser.id)
    {
        webview.open_dev_tools();
    }
    Task::none()
}
