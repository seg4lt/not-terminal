use super::*;

impl App {
    pub(crate) fn add_terminal(&mut self, project_id: &str, worktree_id: &str) -> Option<String> {
        let mut created = None;

        for project in &mut self.persisted.projects {
            if project.id != project_id {
                continue;
            }

            for worktree in &mut project.worktrees {
                if worktree.id != worktree_id {
                    continue;
                }

                let terminal_id = create_id("terminal");
                let terminal_name = next_terminal_name(&worktree.terminals);
                worktree.terminals.push(TerminalRecord {
                    id: terminal_id.clone(),
                    name: terminal_name,
                    manual_name: false,
                });
                project.selected_terminal_id = Some(terminal_id.clone());
                created = Some(terminal_id);
                break;
            }
        }

        // Initialize terminal status after the loop to avoid borrow issues
        if let Some(ref terminal_id) = created {
            self.set_terminal_status(terminal_id.clone(), TerminalStatus::Running);
        }

        self.normalize_selection();
        created
    }

    pub(crate) fn add_detached_terminal(&mut self) -> String {
        let terminal_id = create_id("terminal");
        let terminal_name = next_terminal_name(&self.persisted.detached_terminals);
        self.persisted.detached_terminals.push(TerminalRecord {
            id: terminal_id.clone(),
            name: terminal_name,
            manual_name: false,
        });
        self.persisted.selected_detached_terminal_id = Some(terminal_id.clone());
        // Initialize terminal status as Running
        self.set_terminal_status(terminal_id.clone(), TerminalStatus::Running);
        self.normalize_selection();
        terminal_id
    }

    pub(crate) fn select_detached_terminal(&mut self, terminal_id: &str) {
        if self
            .persisted
            .detached_terminals
            .iter()
            .any(|terminal| terminal.id == terminal_id)
        {
            self.persisted.selected_detached_terminal_id = Some(terminal_id.to_string());
        }
        self.normalize_selection();
    }

    pub(crate) fn remove_terminal(
        &mut self,
        project_id: &str,
        worktree_id: &str,
        terminal_id: &str,
    ) {
        for project in &mut self.persisted.projects {
            if project.id != project_id {
                continue;
            }

            for worktree in &mut project.worktrees {
                if worktree.id != worktree_id {
                    continue;
                }

                worktree
                    .terminals
                    .retain(|terminal| terminal.id != terminal_id);
            }

            if project
                .selected_terminal_id
                .as_ref()
                .is_some_and(|selected| selected == terminal_id)
            {
                project.selected_terminal_id = project.worktrees.iter().find_map(|worktree| {
                    worktree
                        .terminals
                        .first()
                        .map(|terminal| terminal.id.clone())
                });
            }
        }

        self.remove_runtime(terminal_id);
        self.normalize_selection();
    }

    pub(crate) fn remove_detached_terminal(&mut self, terminal_id: &str) {
        self.persisted
            .detached_terminals
            .retain(|terminal| terminal.id != terminal_id);

        if self
            .persisted
            .selected_detached_terminal_id
            .as_ref()
            .is_some_and(|selected| selected == terminal_id)
        {
            self.persisted.selected_detached_terminal_id = self
                .persisted
                .detached_terminals
                .first()
                .map(|terminal| terminal.id.clone());
        }

        self.remove_runtime(terminal_id);
        self.normalize_selection();
    }

    pub(crate) fn close_active_terminal(&mut self) -> bool {
        let Some(active_terminal_id) = self.active_terminal_id() else {
            return false;
        };

        if let Some(locator) = self.find_terminal_locator(&active_terminal_id) {
            let project_id = self.persisted.projects[locator.project_idx].id.clone();
            let worktree_id = self.persisted.projects[locator.project_idx].worktrees
                [locator.worktree_idx]
                .id
                .clone();
            self.remove_terminal(&project_id, &worktree_id, &active_terminal_id);
            return true;
        }

        if self
            .persisted
            .detached_terminals
            .iter()
            .any(|terminal| terminal.id == active_terminal_id)
        {
            self.remove_detached_terminal(&active_terminal_id);
            return true;
        }

        false
    }
}
