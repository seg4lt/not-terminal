const SEARCH_PANE_BUNDLE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/project_search_bundle.js"));

pub(crate) fn render_shell_html(worktree_path: &str) -> String {
    let bootstrap = serde_json::json!({
        "worktreePath": worktree_path,
    });
    let bootstrap_json = serde_json::to_string(&bootstrap)
        .unwrap_or_else(|_| String::from("{\"worktreePath\":\"\"}"))
        .replace("</", "<\\/");
    let bundle = SEARCH_PANE_BUNDLE.replace("</script>", "<\\/script>");

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Project Search</title>
</head>
<body>
<script>window.__NOT_TERMINAL_SEARCH_BOOTSTRAP__ = {};</script>
<script>{}</script>
</body>
</html>"#,
        bootstrap_json, bundle
    )
}
