use super::*;

impl App {
    pub(crate) fn set_terminal_progress_active(&mut self, terminal_id: &str, active: bool) {
        if active {
            self.terminal_progress_active
                .insert(terminal_id.to_string());
        } else {
            self.terminal_progress_active.remove(terminal_id);
        }
    }

    pub(crate) fn is_terminal_progress_active(&self, terminal_id: &str) -> bool {
        self.terminal_progress_active.contains(terminal_id)
    }

    pub(crate) fn advance_terminal_activity_frame(&mut self) {
        self.terminal_activity_frame = (self.terminal_activity_frame + 1) % 4;
    }

    pub(crate) fn terminal_exists(&self, terminal_id: &str) -> bool {
        self.find_terminal_locator(terminal_id).is_some()
            || self
                .persisted
                .detached_terminals
                .iter()
                .any(|terminal| terminal.id == terminal_id)
    }

    /// Get the current status of a terminal
    pub(crate) fn get_terminal_status(&self, terminal_id: &str) -> TerminalStatus {
        self.terminal_status
            .get(terminal_id)
            .cloned()
            .unwrap_or(TerminalStatus::Running)
    }

    /// Set the status of a terminal
    pub(crate) fn set_terminal_status(&mut self, terminal_id: String, status: TerminalStatus) {
        self.terminal_status.insert(terminal_id, status);
    }

    /// Mark a terminal as awaiting response (bell was rung)
    pub(crate) fn on_terminal_bell(&mut self, terminal_id: String) {
        self.terminal_awaiting_response.insert(terminal_id.clone());
        self.terminal_status
            .insert(terminal_id, TerminalStatus::AwaitingResponse);
    }

    /// Clear awaiting response state when user provides input
    pub(crate) fn clear_awaiting_on_activity(&mut self, terminal_id: &str) {
        self.terminal_awaiting_response.remove(terminal_id);
        // If the terminal was in AwaitingResponse state, change it back to Running
        if self.terminal_status.get(terminal_id) == Some(&TerminalStatus::AwaitingResponse) {
            self.terminal_status
                .insert(terminal_id.to_string(), TerminalStatus::Running);
        }
    }

    /// Check if a terminal is awaiting response
    pub(crate) fn is_awaiting_response(&self, terminal_id: &str) -> bool {
        self.terminal_awaiting_response.contains(terminal_id)
    }

    /// Clean up status when terminal is removed
    pub(crate) fn remove_terminal_status(&mut self, terminal_id: &str) {
        self.terminal_status.remove(terminal_id);
        self.terminal_awaiting_response.remove(terminal_id);
        self.terminal_progress_active.remove(terminal_id);
        self.terminal_titles.remove(terminal_id);
    }

    /// Handle title change from Ghostty, checking for bell emoji
    pub(crate) fn on_terminal_title(&mut self, surface_ptr: usize, title: String) {
        // Find which terminal owns this surface by searching all runtimes
        let terminal_id = self
            .runtimes
            .iter()
            .find(|(_, runtime)| runtime.pane_id_for_surface(surface_ptr).is_some())
            .map(|(tid, _)| tid.clone());

        let terminal_id = match terminal_id {
            Some(tid) => tid,
            None => return,
        };

        // Check if title contains bell emoji - set awaiting state if so
        // Note: We DON'T clear the state when emoji is removed - only keyboard input should clear it
        if title.contains('🔔') {
            self.on_terminal_bell(terminal_id.clone());
        }

        // Store title for future reference
        self.terminal_titles.insert(terminal_id, title);
    }
}
