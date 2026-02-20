use super::*;

impl App {
    pub(crate) fn add_browser(&mut self) -> String {
        let browser_id = create_id("browser");
        let browser_name = next_browser_name(&self.persisted.browsers);
        self.persisted.browsers.push(BrowserRecord {
            id: browser_id.clone(),
            name: browser_name,
            url: String::from("https://"),
        });
        self.persisted.selected_browser_id = Some(browser_id.clone());
        self.normalize_selection();
        browser_id
    }

    pub(crate) fn select_browser(&mut self, browser_id: &str) {
        if self
            .persisted
            .browsers
            .iter()
            .any(|browser| browser.id == browser_id)
        {
            self.persisted.selected_browser_id = Some(browser_id.to_string());
        }
        self.normalize_selection();
    }

    pub(crate) fn remove_browser(&mut self, browser_id: &str) {
        self.persisted
            .browsers
            .retain(|browser| browser.id != browser_id);

        if self
            .persisted
            .selected_browser_id
            .as_ref()
            .is_some_and(|selected| selected == browser_id)
        {
            self.persisted.selected_browser_id = self
                .persisted
                .browsers
                .first()
                .map(|browser| browser.id.clone());
        }

        self.browser_webviews.remove(browser_id);
        self.normalize_selection();
    }

    pub(crate) fn active_browser_id(&self) -> Option<String> {
        self.persisted.selected_browser_id.clone()
    }

    pub(crate) fn active_browser(&self) -> Option<&BrowserRecord> {
        let browser_id = self.active_browser_id()?;
        self.persisted
            .browsers
            .iter()
            .find(|browser| browser.id == browser_id)
    }
}
