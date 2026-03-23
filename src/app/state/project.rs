use super::*;
use crate::app::git_branch::resolve_branch;
use crate::app::git_worktrees::{normalize_git_folder_path, resolve_project_identity};

impl App {
    fn rescan_project_internal(&mut self, project_id: &str) -> Result<bool, String> {
        let Some(project_idx) = self
            .persisted
            .projects
            .iter()
            .position(|project| project.id == project_id)
        else {
            return Ok(false);
        };

        let project_before = self.persisted.projects[project_idx].clone();

        let git_folder = self.persisted.projects[project_idx]
            .git_folder_path
            .clone()
            .ok_or_else(|| String::from("Project has no git folder configured"))?;

        let scanned = scan_worktrees(&git_folder)?;

        let project = &mut self.persisted.projects[project_idx];

        let mut existing_worktrees =
            HashMap::<String, (Vec<TerminalRecord>, String, bool, String)>::new();
        for worktree in &project.worktrees {
            existing_worktrees.insert(
                worktree.path.clone(),
                (
                    worktree.terminals.clone(),
                    worktree.name.clone(),
                    worktree.manual_name,
                    worktree.id.clone(),
                ),
            );
        }

        let active_before = project.selected_terminal_id.clone();
        let mut next_worktrees = Vec::new();
        let mut seen = HashSet::new();

        for info in scanned {
            seen.insert(info.path.clone());
            let (terminals, name, manual_name, id) = existing_worktrees
                .remove(&info.path)
                .unwrap_or((Vec::new(), info.name.clone(), false, info.id.clone()));
            next_worktrees.push(WorktreeRecord {
                id,
                name: if manual_name { name } else { info.name },
                manual_name,
                path: info.path,
                missing: info.missing,
                terminals,
            });
        }

        let mut removed_terminal_ids = Vec::new();
        for worktree in &project.worktrees {
            if seen.contains(&worktree.path) {
                continue;
            }
            removed_terminal_ids.extend(
                worktree
                    .terminals
                    .iter()
                    .map(|terminal| terminal.id.clone()),
            );
        }

        project.worktrees = next_worktrees;

        if let Some(selected) = active_before
            && project_terminal_exists(project, &selected)
        {
            project.selected_terminal_id = Some(selected);
        } else {
            project.selected_terminal_id = project.worktrees.iter().find_map(|worktree| {
                worktree
                    .terminals
                    .first()
                    .map(|terminal| terminal.id.clone())
            });
        }

        for terminal_id in removed_terminal_ids {
            self.remove_runtime(&terminal_id);
        }

        self.resolve_and_update_worktree_branches(project_id);
        self.normalize_selection();

        Ok(self.persisted.projects[project_idx] != project_before)
    }

    pub(crate) fn add_project_from_git_folder(
        &mut self,
        git_folder: &str,
    ) -> Result<AddProjectOutcome, String> {
        let normalized_git_folder = normalize_git_folder_path(git_folder)?;
        let project_identity = resolve_project_identity(&normalized_git_folder)?;

        if let Some((existing_project_id, existing_project_name)) =
            self.persisted.projects.iter().find_map(|project| {
                let Some(existing_git_folder) = project.git_folder_path.as_deref() else {
                    return None;
                };

                let matches_existing = resolve_project_identity(existing_git_folder)
                    .map(|existing_identity| existing_identity == project_identity)
                    .unwrap_or_else(|_| {
                        normalize_git_folder_path(existing_git_folder)
                            .map(|existing_path| existing_path == normalized_git_folder)
                            .unwrap_or(existing_git_folder == normalized_git_folder)
                    });

                matches_existing.then(|| (project.id.clone(), project.name.clone()))
            })
        {
            self.persisted.active_project_id = Some(existing_project_id);
            self.normalize_selection();
            return Ok(AddProjectOutcome::AlreadyExists {
                project_name: existing_project_name,
            });
        }

        let scanned = scan_worktrees(&normalized_git_folder)?;

        let project_id = create_id("project");
        let mut project = ProjectRecord {
            id: project_id.clone(),
            name: infer_project_name(&normalized_git_folder),
            git_folder_path: Some(normalized_git_folder.clone()),
            worktrees: scanned
                .into_iter()
                .map(|worktree| WorktreeRecord {
                    id: worktree.id,
                    name: worktree.name,
                    manual_name: false,
                    path: worktree.path,
                    missing: worktree.missing,
                    terminals: Vec::new(),
                })
                .collect(),
            tree_state: TreeStateRecord::default(),
            selected_terminal_id: None,
        };

        if project.name.trim().is_empty() {
            project.name = next_project_name(&self.persisted.projects);
        }

        self.persisted.projects.push(project);
        self.persisted.active_project_id = Some(project_id.clone());

        // Proactively resolve branches and update worktree names
        self.resolve_and_update_worktree_branches(&project_id);

        self.normalize_selection();
        Ok(AddProjectOutcome::Added {
            path: normalized_git_folder,
        })
    }

    fn resolve_and_update_worktree_branches(&mut self, project_id: &str) {
        let project_idx = if let Some(idx) = self
            .persisted
            .projects
            .iter()
            .position(|project| project.id == project_id)
        {
            idx
        } else {
            return;
        };

        for worktree in &mut self.persisted.projects[project_idx].worktrees {
            if let Some(branch) = resolve_branch(&worktree.path) {
                // Update worktree name to branch name, unless it was manually renamed
                if !worktree.manual_name {
                    worktree.name = branch;
                }
            }
        }
    }

    pub(crate) fn rescan_project(&mut self, project_id: &str) -> Result<(), String> {
        self.rescan_project_internal(project_id).map(|_| ())
    }

    pub(crate) fn rescan_all_projects(&mut self) -> ProjectRescanSummary {
        let project_refs: Vec<(String, String)> = self
            .persisted
            .projects
            .iter()
            .map(|project| (project.id.clone(), project.name.clone()))
            .collect();

        let mut summary = ProjectRescanSummary {
            total_projects: project_refs.len(),
            ..ProjectRescanSummary::default()
        };

        for (project_id, project_name) in project_refs {
            match self.rescan_project_internal(&project_id) {
                Ok(changed) => {
                    summary.successful_projects += 1;
                    if changed {
                        summary.changed_projects += 1;
                    }
                }
                Err(error) => {
                    summary
                        .failed_projects
                        .push(format!("{project_name}: {error}"));
                }
            }
        }

        summary
    }

    pub(crate) fn start_add_worktree(&mut self, project_id: &str) {
        let Some(project) = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == project_id)
        else {
            return;
        };

        let Some(git_folder) = project.git_folder_path.as_ref() else {
            return;
        };

        let branch_name = String::from("feature");
        let destination_path = suggest_worktree_destination(git_folder, &branch_name);
        self.add_worktree_dialog = Some(AddWorktreeDialog {
            project_id: project_id.to_string(),
            branch_name,
            destination_path,
        });
    }

    pub(crate) fn commit_add_worktree(&mut self) -> Result<(), String> {
        let Some(dialog) = self.add_worktree_dialog.clone() else {
            return Ok(());
        };

        let branch_name = dialog.branch_name.trim();
        let destination_path = dialog.destination_path.trim();
        if branch_name.is_empty() {
            return Err(String::from("Branch name cannot be empty"));
        }
        if destination_path.is_empty() {
            return Err(String::from("Destination path cannot be empty"));
        }

        let Some(project) = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == dialog.project_id)
        else {
            return Err(String::from("Project not found"));
        };

        let git_folder = project
            .git_folder_path
            .as_ref()
            .ok_or_else(|| String::from("Project has no git folder configured"))?;

        add_worktree(git_folder, destination_path, branch_name)?;
        self.add_worktree_dialog = None;
        self.rescan_project(&dialog.project_id)?;
        Ok(())
    }

    pub(crate) fn suggested_worktree_destination(
        &self,
        project_id: &str,
        branch_name: &str,
    ) -> Option<String> {
        let project = self
            .persisted
            .projects
            .iter()
            .find(|project| project.id == project_id)?;
        let git_folder = project.git_folder_path.as_ref()?;
        Some(suggest_worktree_destination(git_folder, branch_name))
    }

    pub(crate) fn remove_worktree(
        &mut self,
        project_id: &str,
        worktree_id: &str,
    ) -> Result<(), String> {
        let Some(project_idx) = self
            .persisted
            .projects
            .iter()
            .position(|project| project.id == project_id)
        else {
            return Ok(());
        };

        let git_folder = self.persisted.projects[project_idx]
            .git_folder_path
            .clone()
            .ok_or_else(|| String::from("Project has no git folder configured"))?;

        let worktree = self.persisted.projects[project_idx]
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .ok_or_else(|| String::from("Worktree not found"))?;

        if worktree.path == git_folder {
            return Err(String::from("Main worktree cannot be removed"));
        }

        remove_worktree(&git_folder, &worktree.path)?;
        self.rescan_project(project_id)?;
        Ok(())
    }

    pub(crate) fn toggle_project_collapsed(&mut self, project_id: &str) {
        if let Some(project) = self
            .persisted
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            toggle_in_list(&mut project.tree_state.collapsed_projects, project_id);
        }
    }

    pub(crate) fn reorder_project(
        &mut self,
        dragged_project_id: &str,
        target_project_id: &str,
    ) -> bool {
        move_vec_item_by(
            &mut self.persisted.projects,
            |project| project.id == dragged_project_id,
            |project| project.id == target_project_id,
        )
    }

    pub(crate) fn all_project_trees_expanded(&self) -> bool {
        let mut has_any_projects = false;

        for project in &self.persisted.projects {
            has_any_projects = true;

            if crate::app::state::App::project_collapsed(project) {
                return false;
            }

            for worktree in &project.worktrees {
                if project
                    .tree_state
                    .collapsed_worktrees
                    .iter()
                    .any(|id| id == &worktree.id)
                {
                    return false;
                }
            }
        }

        has_any_projects
    }

    pub(crate) fn all_project_trees_collapsed(&self) -> bool {
        let mut has_any_projects = false;

        for project in &self.persisted.projects {
            has_any_projects = true;

            if !crate::app::state::App::project_collapsed(project) {
                return false;
            }

            for worktree in &project.worktrees {
                if !project
                    .tree_state
                    .collapsed_worktrees
                    .iter()
                    .any(|id| id == &worktree.id)
                {
                    return false;
                }
            }
        }

        has_any_projects
    }

    pub(crate) fn collapse_all_project_trees(&mut self) {
        for project in &mut self.persisted.projects {
            project.tree_state.collapsed_projects = vec![project.id.clone()];
            project.tree_state.collapsed_worktrees = project
                .worktrees
                .iter()
                .map(|worktree| worktree.id.clone())
                .collect();
        }
    }

    pub(crate) fn expand_all_project_trees(&mut self) {
        for project in &mut self.persisted.projects {
            project.tree_state.collapsed_worktrees.clear();
            project.tree_state.collapsed_projects.clear();
        }
    }

    pub(crate) fn toggle_all_project_trees_collapsed(&mut self) {
        let collapse_all = self.all_project_trees_expanded();

        if collapse_all {
            self.collapse_all_project_trees();
        } else {
            self.expand_all_project_trees();
        }
    }

    pub(crate) fn toggle_worktree_collapsed(&mut self, project_id: &str, worktree_id: &str) {
        if let Some(project) = self
            .persisted
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            toggle_in_list(&mut project.tree_state.collapsed_worktrees, worktree_id);
        }
    }

    pub(crate) fn reorder_worktree(
        &mut self,
        project_id: &str,
        dragged_worktree_id: &str,
        target_worktree_id: &str,
    ) -> bool {
        let Some(project) = self
            .persisted
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        else {
            return false;
        };

        move_vec_item_by(
            &mut project.worktrees,
            |worktree| worktree.id == dragged_worktree_id,
            |worktree| worktree.id == target_worktree_id,
        )
    }

    pub(crate) fn reorder_terminal(
        &mut self,
        project_id: &str,
        worktree_id: &str,
        dragged_terminal_id: &str,
        target_terminal_id: &str,
    ) -> bool {
        let Some(project) = self
            .persisted
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        else {
            return false;
        };

        let Some(worktree) = project
            .worktrees
            .iter_mut()
            .find(|worktree| worktree.id == worktree_id)
        else {
            return false;
        };

        move_vec_item_by(
            &mut worktree.terminals,
            |terminal| terminal.id == dragged_terminal_id,
            |terminal| terminal.id == target_terminal_id,
        )
    }

    pub(crate) fn remove_project(&mut self, project_id: &str) -> Result<(), String> {
        let project_idx = self
            .persisted
            .projects
            .iter()
            .position(|project| project.id == project_id)
            .ok_or_else(|| String::from("Project not found"))?;

        let project = &self.persisted.projects[project_idx];

        let terminal_ids: Vec<String> = project
            .worktrees
            .iter()
            .flat_map(|worktree| {
                worktree
                    .terminals
                    .iter()
                    .map(|terminal| terminal.id.clone())
            })
            .collect();

        for terminal_id in terminal_ids {
            self.remove_runtime(&terminal_id);
        }

        self.persisted.projects.remove(project_idx);

        self.normalize_selection();

        Ok(())
    }
}
