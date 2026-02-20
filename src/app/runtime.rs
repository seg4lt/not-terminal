use crate::ghostty_embed::{
    GhosttyEmbed, GhosttyGotoSplitDirection, GhosttyResizeSplitDirection, GhosttyRuntimeAction,
    GhosttySplitDirection, host_view_free, host_view_set_frame, host_view_set_hidden,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
enum SplitNode {
    Leaf(String),
    Branch {
        axis: SplitAxis,
        ratio: f32,
        first: Box<SplitNode>,
        second: Box<SplitNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PaneRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TickOutcome {
    pub(crate) had_pending_work: bool,
    pub(crate) layout_changed: bool,
}

pub(crate) struct PaneRuntime {
    pub(crate) id: String,
    pub(crate) host_view: usize,
    pub(crate) ghostty: GhosttyEmbed,
    last_frame: Option<(f64, f64, f64, f64)>,
    last_size_px: Option<(u32, u32)>,
    last_scale: Option<f64>,
    last_focus: Option<bool>,
    last_hidden: Option<bool>,
}

impl PaneRuntime {
    pub(crate) fn new(id: String, host_view: usize, ghostty: GhosttyEmbed) -> Self {
        Self {
            id,
            host_view,
            ghostty,
            last_frame: None,
            last_size_px: None,
            last_scale: None,
            last_focus: None,
            last_hidden: None,
        }
    }

    pub(crate) fn surface_ptr(&self) -> usize {
        self.ghostty.surface_ptr()
    }
}

impl Drop for PaneRuntime {
    fn drop(&mut self) {
        host_view_free(self.host_view);
    }
}

pub(crate) struct RuntimeSession {
    panes: HashMap<String, PaneRuntime>,
    root: SplitNode,
    active_pane_id: String,
    zoomed_pane_id: Option<String>,
}

impl RuntimeSession {
    pub(crate) fn new(initial_pane: PaneRuntime) -> Self {
        let root_id = initial_pane.id.clone();
        let mut panes = HashMap::new();
        panes.insert(root_id.clone(), initial_pane);

        Self {
            panes,
            root: SplitNode::Leaf(root_id.clone()),
            active_pane_id: root_id,
            zoomed_pane_id: None,
        }
    }

    pub(crate) fn active_ghostty_mut(&mut self) -> Option<&mut GhosttyEmbed> {
        self.panes
            .get_mut(&self.active_pane_id)
            .map(|pane| &mut pane.ghostty)
    }

    pub(crate) fn tick_all(&mut self) -> TickOutcome {
        // Fast path: check if ANY pane has pending work before iterating
        // This avoids FFI calls when no terminal has data to process
        let has_any_pending = self
            .panes
            .values()
            .any(|pane| pane.ghostty.has_pending_tick());

        if !has_any_pending {
            // No pending work anywhere - still need to check for exited panes
            let exited: Vec<String> = self
                .panes
                .iter()
                .filter(|(_, pane)| pane.ghostty.process_exited())
                .map(|(pane_id, _)| pane_id.clone())
                .collect();

            if exited.is_empty() {
                // No pending work and no exited panes - early exit
                return TickOutcome::default();
            }

            // Handle exited panes only
            let mut changed = false;
            for pane_id in exited {
                if self.panes.len() > 1 {
                    changed |= self.remove_pane(&pane_id);
                }
            }

            if changed {
                self.clear_active_input_modes();
            }
            return TickOutcome {
                had_pending_work: false,
                layout_changed: changed,
            };
        }

        // At least one pane has pending work - process all panes
        let mut had_pending_work = false;
        for pane in self.panes.values_mut() {
            if pane.ghostty.has_pending_tick() {
                had_pending_work = true;
            }
            pane.ghostty.tick_if_needed();
        }

        // Auto-remove split panes that have exited (but keep at least one pane)
        let exited: Vec<String> = self
            .panes
            .iter()
            .filter(|(_, pane)| pane.ghostty.process_exited())
            .map(|(pane_id, _)| pane_id.clone())
            .collect();

        let mut changed = false;
        for pane_id in exited {
            // Only remove if there's more than one pane (i.e., it's a split)
            if self.panes.len() > 1 {
                changed |= self.remove_pane(&pane_id);
            }
        }

        if changed {
            self.clear_active_input_modes();
        }

        TickOutcome {
            had_pending_work,
            layout_changed: changed,
        }
    }

    pub(crate) fn drain_actions(&mut self) -> Vec<GhosttyRuntimeAction> {
        let mut actions = Vec::new();
        for pane in self.panes.values_mut() {
            actions.extend(pane.ghostty.drain_actions());
        }
        actions
    }

    pub(crate) fn apply_layout(
        &mut self,
        frame_x: f32,
        frame_y: f32,
        frame_width: f32,
        frame_height: f32,
        visible: bool,
        scale: f64,
    ) {
        if !visible {
            for pane in self.panes.values_mut() {
                if pane.last_hidden != Some(true) {
                    host_view_set_hidden(pane.host_view, true);
                    pane.last_hidden = Some(true);
                }
                if pane.last_focus != Some(false) {
                    pane.ghostty.set_focus(false);
                    pane.last_focus = Some(false);
                }
            }
            return;
        }

        let layout = self.compute_layout(frame_width, frame_height);

        for (pane_id, pane) in &mut self.panes {
            let Some(rect) = layout
                .iter()
                .find(|(id, _)| id == pane_id)
                .map(|(_, rect)| *rect)
            else {
                if pane.last_hidden != Some(true) {
                    host_view_set_hidden(pane.host_view, true);
                    pane.last_hidden = Some(true);
                }
                if pane.last_focus != Some(false) {
                    pane.ghostty.set_focus(false);
                    pane.last_focus = Some(false);
                }
                continue;
            };

            let frame = (
                (frame_x + rect.x) as f64,
                (frame_y + rect.y) as f64,
                rect.width.max(1.0) as f64,
                rect.height.max(1.0) as f64,
            );
            let frame_changed = pane.last_frame != Some(frame);
            if frame_changed {
                host_view_set_frame(pane.host_view, frame.0, frame.1, frame.2, frame.3);
                pane.last_frame = Some(frame);
            }

            let hidden_changed = pane.last_hidden != Some(false);
            if hidden_changed {
                host_view_set_hidden(pane.host_view, false);
                pane.last_hidden = Some(false);
            }

            let width_px = (rect.width.max(1.0) as f64 * scale).round().max(1.0) as u32;
            let height_px = (rect.height.max(1.0) as f64 * scale).round().max(1.0) as u32;
            let scale_changed = pane.last_scale != Some(scale);
            if scale_changed {
                pane.ghostty.set_scale_factor(scale);
                pane.last_scale = Some(scale);
            }

            let size = (width_px, height_px);
            let size_changed = pane.last_size_px != Some(size);
            if size_changed {
                pane.ghostty.set_size(width_px, height_px);
                pane.last_size_px = Some(size);
            }

            let focused = pane_id == &self.active_pane_id;
            let focus_changed = pane.last_focus != Some(focused);
            if focus_changed {
                pane.ghostty.set_focus(focused);
                pane.last_focus = Some(focused);
            }

            if focused
                && (frame_changed
                    || hidden_changed
                    || scale_changed
                    || size_changed
                    || focus_changed)
            {
                pane.ghostty.refresh();
            }
        }
    }

    pub(crate) fn active_pane_local(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Option<(f64, f64)> {
        let layout = self.compute_layout(width, height);
        let (_, rect) = layout
            .iter()
            .find(|(pane_id, _)| pane_id == &self.active_pane_id)?;

        if x < rect.x || x >= rect.x + rect.width || y < rect.y || y >= rect.y + rect.height {
            return None;
        }

        Some(((x - rect.x) as f64, (y - rect.y) as f64))
    }

    pub(crate) fn focus_pane_at(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Option<(f64, f64, bool)> {
        let layout = self.compute_layout(width, height);
        let (pane_id, rect) = layout.iter().find(|(_, rect)| {
            x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
        })?;

        let changed = self.active_pane_id != *pane_id;
        if changed {
            self.active_pane_id = pane_id.clone();
            if self.zoomed_pane_id.is_some() {
                self.zoomed_pane_id = Some(self.active_pane_id.clone());
            }
        }

        Some(((x - rect.x) as f64, (y - rect.y) as f64, changed))
    }

    pub(crate) fn split_from_surface(
        &mut self,
        surface_ptr: usize,
        direction: GhosttySplitDirection,
        new_pane: PaneRuntime,
    ) -> bool {
        let source_id = self
            .pane_id_for_surface(surface_ptr)
            .unwrap_or_else(|| self.active_pane_id.clone());

        if !self.panes.contains_key(&source_id) {
            return false;
        }

        let new_id = new_pane.id.clone();
        let (axis, first, second) = match direction {
            GhosttySplitDirection::Right => (
                SplitAxis::Vertical,
                SplitNode::Leaf(source_id.clone()),
                SplitNode::Leaf(new_id.clone()),
            ),
            GhosttySplitDirection::Down => (
                SplitAxis::Horizontal,
                SplitNode::Leaf(source_id.clone()),
                SplitNode::Leaf(new_id.clone()),
            ),
            GhosttySplitDirection::Left => (
                SplitAxis::Vertical,
                SplitNode::Leaf(new_id.clone()),
                SplitNode::Leaf(source_id.clone()),
            ),
            GhosttySplitDirection::Up => (
                SplitAxis::Horizontal,
                SplitNode::Leaf(new_id.clone()),
                SplitNode::Leaf(source_id.clone()),
            ),
        };

        let replacement = SplitNode::Branch {
            axis,
            ratio: 0.5,
            first: Box::new(first),
            second: Box::new(second),
        };

        if !replace_leaf(&mut self.root, &source_id, replacement) {
            return false;
        }

        self.panes.insert(new_id.clone(), new_pane);
        self.active_pane_id = new_id;
        self.zoomed_pane_id = None;
        true
    }

    pub(crate) fn goto_split_from_surface(
        &mut self,
        surface_ptr: usize,
        direction: GhosttyGotoSplitDirection,
        width: f32,
        height: f32,
    ) -> bool {
        let source_id = self
            .pane_id_for_surface(surface_ptr)
            .unwrap_or_else(|| self.active_pane_id.clone());
        if !self.panes.contains_key(&source_id) {
            return false;
        }

        match direction {
            GhosttyGotoSplitDirection::Previous | GhosttyGotoSplitDirection::Next => {
                let order = in_order_leaf_ids(&self.root);
                if order.len() <= 1 {
                    return false;
                }

                let Some(current) = order.iter().position(|pane_id| pane_id == &source_id) else {
                    return false;
                };

                let next = match direction {
                    GhosttyGotoSplitDirection::Previous => {
                        (current + order.len().saturating_sub(1)) % order.len()
                    }
                    GhosttyGotoSplitDirection::Next => (current + 1) % order.len(),
                    _ => unreachable!(),
                };

                self.active_pane_id = order[next].clone();
                if self.zoomed_pane_id.is_some() {
                    self.zoomed_pane_id = Some(self.active_pane_id.clone());
                }
                true
            }
            GhosttyGotoSplitDirection::Up
            | GhosttyGotoSplitDirection::Left
            | GhosttyGotoSplitDirection::Down
            | GhosttyGotoSplitDirection::Right => {
                let layout = self.compute_layout(width, height);
                let Some((_, source_rect)) =
                    layout.iter().find(|(pane_id, _)| pane_id == &source_id)
                else {
                    return false;
                };

                let source_center_x = source_rect.x + source_rect.width / 2.0;
                let source_center_y = source_rect.y + source_rect.height / 2.0;

                let mut best: Option<(&String, f32)> = None;

                for (candidate_id, candidate_rect) in &layout {
                    if candidate_id == &source_id {
                        continue;
                    }

                    let candidate_center_x = candidate_rect.x + candidate_rect.width / 2.0;
                    let candidate_center_y = candidate_rect.y + candidate_rect.height / 2.0;

                    let (primary, secondary, valid) = match direction {
                        GhosttyGotoSplitDirection::Left => {
                            let valid =
                                candidate_rect.x + candidate_rect.width <= source_rect.x + 0.5;
                            (
                                source_rect.x - (candidate_rect.x + candidate_rect.width),
                                (candidate_center_y - source_center_y).abs(),
                                valid,
                            )
                        }
                        GhosttyGotoSplitDirection::Right => {
                            let valid = candidate_rect.x >= source_rect.x + source_rect.width - 0.5;
                            (
                                candidate_rect.x - (source_rect.x + source_rect.width),
                                (candidate_center_y - source_center_y).abs(),
                                valid,
                            )
                        }
                        GhosttyGotoSplitDirection::Up => {
                            let valid =
                                candidate_rect.y + candidate_rect.height <= source_rect.y + 0.5;
                            (
                                source_rect.y - (candidate_rect.y + candidate_rect.height),
                                (candidate_center_x - source_center_x).abs(),
                                valid,
                            )
                        }
                        GhosttyGotoSplitDirection::Down => {
                            let valid =
                                candidate_rect.y >= source_rect.y + source_rect.height - 0.5;
                            (
                                candidate_rect.y - (source_rect.y + source_rect.height),
                                (candidate_center_x - source_center_x).abs(),
                                valid,
                            )
                        }
                        _ => unreachable!(),
                    };

                    if !valid {
                        continue;
                    }

                    let score = primary.max(0.0) + secondary * 0.25;
                    match best {
                        Some((_, current_score)) if score >= current_score => {}
                        _ => best = Some((candidate_id, score)),
                    }
                }

                let Some((target_id, _)) = best else {
                    return false;
                };

                self.active_pane_id = target_id.clone();
                if self.zoomed_pane_id.is_some() {
                    self.zoomed_pane_id = Some(self.active_pane_id.clone());
                }
                true
            }
        }
    }

    pub(crate) fn resize_split_from_surface(
        &mut self,
        surface_ptr: usize,
        direction: GhosttyResizeSplitDirection,
        amount: u16,
    ) -> bool {
        let source_id = self
            .pane_id_for_surface(surface_ptr)
            .unwrap_or_else(|| self.active_pane_id.clone());
        if !self.panes.contains_key(&source_id) {
            return false;
        }

        let delta = ((amount.max(1) as f32) / 500.0).clamp(0.01, 0.2);
        resize_for_leaf(&mut self.root, &source_id, direction, delta)
    }

    pub(crate) fn equalize_splits(&mut self) -> bool {
        if self.panes.len() <= 1 {
            return false;
        }

        equalize_node(&mut self.root);
        true
    }

    pub(crate) fn toggle_split_zoom_from_surface(&mut self, surface_ptr: usize) -> bool {
        if self.panes.len() <= 1 {
            return false;
        }

        let source_id = self
            .pane_id_for_surface(surface_ptr)
            .unwrap_or_else(|| self.active_pane_id.clone());
        if !self.panes.contains_key(&source_id) {
            return false;
        }

        self.active_pane_id = source_id.clone();
        if self.zoomed_pane_id.as_ref() == Some(&source_id) {
            self.zoomed_pane_id = None;
        } else {
            self.zoomed_pane_id = Some(source_id);
        }
        true
    }

    pub(crate) fn pane_id_for_surface(&self, surface_ptr: usize) -> Option<String> {
        if surface_ptr == 0 {
            return None;
        }

        self.panes
            .values()
            .find(|pane| pane.surface_ptr() == surface_ptr)
            .map(|pane| pane.id.clone())
    }

    #[allow(dead_code)]
    fn remove_pane(&mut self, pane_id: &str) -> bool {
        if !self.panes.contains_key(pane_id) || self.panes.len() <= 1 {
            return false;
        }

        let (next_root, removed) = remove_leaf_from_tree(&self.root, pane_id);
        if !removed {
            return false;
        }

        let Some(next_root) = next_root else {
            return false;
        };

        self.root = next_root;
        self.panes.remove(pane_id);

        if self.active_pane_id == pane_id {
            if let Some(next_active) = in_order_leaf_ids(&self.root).first() {
                self.active_pane_id = next_active.clone();
            }
        }

        if self.zoomed_pane_id.as_deref() == Some(pane_id) {
            self.zoomed_pane_id = None;
        }

        true
    }

    #[allow(dead_code)]
    fn clear_active_input_modes(&mut self) {
        let Some(active) = self.active_ghostty_mut() else {
            return;
        };

        // Defensive reset for intermittent "stuck" leader/key-table state
        // after split lifecycle changes.
        let _ = active.binding_action("end_key_sequence");
        let _ = active.binding_action("deactivate_all_key_tables");
        active.refresh();
        active.force_tick();
    }

    fn compute_layout(&self, width: f32, height: f32) -> Vec<(String, PaneRect)> {
        if let Some(zoomed_id) = self
            .zoomed_pane_id
            .as_ref()
            .filter(|pane_id| self.panes.contains_key(*pane_id))
        {
            return vec![(
                zoomed_id.clone(),
                PaneRect {
                    x: 0.0,
                    y: 0.0,
                    width: width.max(1.0),
                    height: height.max(1.0),
                },
            )];
        }

        let mut result = Vec::new();
        collect_layout(
            &self.root,
            0.0,
            0.0,
            width.max(1.0),
            height.max(1.0),
            &mut result,
        );
        result
    }
}

fn replace_leaf(node: &mut SplitNode, target: &str, replacement: SplitNode) -> bool {
    match node {
        SplitNode::Leaf(id) => {
            if id == target {
                *node = replacement;
                true
            } else {
                false
            }
        }
        SplitNode::Branch { first, second, .. } => {
            replace_leaf(first, target, replacement.clone())
                || replace_leaf(second, target, replacement)
        }
    }
}

fn collect_layout(
    node: &SplitNode,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    result: &mut Vec<(String, PaneRect)>,
) {
    match node {
        SplitNode::Leaf(id) => {
            result.push((
                id.clone(),
                PaneRect {
                    x,
                    y,
                    width,
                    height,
                },
            ));
        }
        SplitNode::Branch {
            axis,
            ratio,
            first,
            second,
        } => match axis {
            SplitAxis::Vertical => {
                let mut first_width = (width * ratio.clamp(0.15, 0.85)).round();
                first_width = first_width.max(1.0).min((width - 1.0).max(1.0));
                let second_width = (width - first_width).max(1.0);
                collect_layout(first, x, y, first_width, height, result);
                collect_layout(second, x + first_width, y, second_width, height, result);
            }
            SplitAxis::Horizontal => {
                let mut first_height = (height * ratio.clamp(0.15, 0.85)).round();
                first_height = first_height.max(1.0).min((height - 1.0).max(1.0));
                let second_height = (height - first_height).max(1.0);
                collect_layout(first, x, y, width, first_height, result);
                collect_layout(second, x, y + first_height, width, second_height, result);
            }
        },
    }
}

fn in_order_leaf_ids(node: &SplitNode) -> Vec<String> {
    let mut ids = Vec::new();
    collect_leaf_ids(node, &mut ids);
    ids
}

fn collect_leaf_ids(node: &SplitNode, ids: &mut Vec<String>) {
    match node {
        SplitNode::Leaf(id) => ids.push(id.clone()),
        SplitNode::Branch { first, second, .. } => {
            collect_leaf_ids(first, ids);
            collect_leaf_ids(second, ids);
        }
    }
}

fn contains_leaf(node: &SplitNode, target: &str) -> bool {
    match node {
        SplitNode::Leaf(id) => id == target,
        SplitNode::Branch { first, second, .. } => {
            contains_leaf(first, target) || contains_leaf(second, target)
        }
    }
}

fn resize_for_leaf(
    node: &mut SplitNode,
    target_leaf: &str,
    direction: GhosttyResizeSplitDirection,
    delta: f32,
) -> bool {
    let SplitNode::Branch {
        axis,
        ratio,
        first,
        second,
    } = node
    else {
        return false;
    };

    let in_first = contains_leaf(first, target_leaf);
    let in_second = contains_leaf(second, target_leaf);
    if !in_first && !in_second {
        return false;
    }

    if resize_for_leaf(first, target_leaf, direction, delta)
        || resize_for_leaf(second, target_leaf, direction, delta)
    {
        return true;
    }

    let adjustment = match axis {
        SplitAxis::Vertical => match direction {
            GhosttyResizeSplitDirection::Right => {
                if in_first {
                    delta
                } else {
                    -delta
                }
            }
            GhosttyResizeSplitDirection::Left => {
                if in_first {
                    -delta
                } else {
                    delta
                }
            }
            GhosttyResizeSplitDirection::Up | GhosttyResizeSplitDirection::Down => return false,
        },
        SplitAxis::Horizontal => match direction {
            GhosttyResizeSplitDirection::Down => {
                if in_first {
                    delta
                } else {
                    -delta
                }
            }
            GhosttyResizeSplitDirection::Up => {
                if in_first {
                    -delta
                } else {
                    delta
                }
            }
            GhosttyResizeSplitDirection::Left | GhosttyResizeSplitDirection::Right => return false,
        },
    };

    *ratio = (*ratio + adjustment).clamp(0.15, 0.85);
    true
}

fn equalize_node(node: &mut SplitNode) {
    match node {
        SplitNode::Leaf(_) => {}
        SplitNode::Branch {
            ratio,
            first,
            second,
            ..
        } => {
            *ratio = 0.5;
            equalize_node(first);
            equalize_node(second);
        }
    }
}

#[allow(dead_code)]
fn remove_leaf_from_tree(node: &SplitNode, target: &str) -> (Option<SplitNode>, bool) {
    match node {
        SplitNode::Leaf(id) => {
            if id == target {
                (None, true)
            } else {
                (Some(SplitNode::Leaf(id.clone())), false)
            }
        }
        SplitNode::Branch {
            axis,
            ratio,
            first,
            second,
        } => {
            let (first_next, first_removed) = remove_leaf_from_tree(first, target);
            let (second_next, second_removed) = remove_leaf_from_tree(second, target);
            let removed = first_removed || second_removed;

            let next = match (first_next, second_next) {
                (Some(first), Some(second)) => Some(SplitNode::Branch {
                    axis: *axis,
                    ratio: *ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(child), None) | (None, Some(child)) => Some(child),
                (None, None) => None,
            };

            (next, removed)
        }
    }
}
