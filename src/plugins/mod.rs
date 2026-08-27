pub mod git;

use ratatui::{Frame, layout::Rect};

pub trait Plugin {
    fn name(&self) -> &str;
    fn render(&mut self, f: &mut Frame, area: Rect);
}

pub struct PluginRegistry {
    pub plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name()).collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
