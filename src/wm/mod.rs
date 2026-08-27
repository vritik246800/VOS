#![allow(dead_code)]
pub mod layout;

use crate::core::state::AppMode;
use layout::WindowLayout;

#[derive(Debug)]
pub struct Window {
    pub id: usize,
    pub title: String,
    pub mode: AppMode,
    pub layout: WindowLayout,
    pub focused_pane: usize,
}

impl Window {
    pub fn new(id: usize, title: impl Into<String>, mode: AppMode) -> Self {
        Self {
            id,
            title: title.into(),
            mode,
            layout: WindowLayout::Monocle,
            focused_pane: 0,
        }
    }
}

pub struct WindowManager {
    pub windows: Vec<Window>,
    pub active: usize,
    pub focus_history: Vec<usize>,
    next_id: usize,
}

impl WindowManager {
    pub fn new() -> Self {
        let mut wm = Self {
            windows: Vec::new(),
            active: 0,
            focus_history: Vec::new(),
            next_id: 0,
        };
        wm.push(Window::new(0, "Menu", AppMode::Menu));
        wm
    }

    pub fn push(&mut self, window: Window) {
        self.windows.push(window);
        self.next_id += 1;
    }

    pub fn active_window(&self) -> Option<&Window> {
        self.windows.get(self.active)
    }

    pub fn active_window_mut(&mut self) -> Option<&mut Window> {
        self.windows.get_mut(self.active)
    }

    pub fn focus(&mut self, idx: usize) {
        if idx < self.windows.len() {
            self.focus_history.push(self.active);
            if self.focus_history.len() > 20 {
                self.focus_history.remove(0);
            }
            self.active = idx;
        }
    }

    pub fn focus_prev(&mut self) {
        if let Some(prev) = self.focus_history.pop() {
            self.active = prev;
        }
    }

    pub fn open(&mut self, title: impl Into<String>, mode: AppMode) -> usize {
        let id = self.next_id;
        self.push(Window::new(id, title, mode));
        let idx = self.windows.len() - 1;
        self.focus(idx);
        idx
    }

    pub fn close_active(&mut self) {
        if self.windows.len() > 1 {
            self.windows.remove(self.active);
            self.active = self.active.saturating_sub(1);
        }
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}
