use crate::ghostty_embed::{GhosttyEmbed, host_view_free};

pub(crate) struct RuntimeSession {
    pub(crate) host_view: usize,
    pub(crate) ghostty: GhosttyEmbed,
}

impl RuntimeSession {
    pub(crate) fn new(host_view: usize, ghostty: GhosttyEmbed) -> Self {
        Self { host_view, ghostty }
    }
}

impl Drop for RuntimeSession {
    fn drop(&mut self) {
        host_view_free(self.host_view);
    }
}
