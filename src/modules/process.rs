use ratatui::widgets::ListState;
use sysinfo::System;

#[derive(Debug, Clone)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem_mb: f64,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortBy {
    Cpu,
    Memory,
    Pid,
    Name,
}

pub struct ProcessViewer {
    pub processes: Vec<ProcessEntry>,
    pub list_state: ListState,
    pub sort_by: SortBy,
    pub filter: String,
    pub filtered: Vec<usize>,
    sys: System,
}

impl ProcessViewer {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let mut pv = Self {
            processes: Vec::new(),
            list_state: ListState::default(),
            sort_by: SortBy::Cpu,
            filter: String::new(),
            filtered: Vec::new(),
            sys,
        };
        pv.refresh();
        pv
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_all();
        self.processes = self
            .sys
            .processes()
            .values()
            .map(|p| ProcessEntry {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu: p.cpu_usage(),
                mem_mb: p.memory() as f64 / 1024.0 / 1024.0,
                status: format!("{:?}", p.status()),
            })
            .collect();

        self.sort();
        self.apply_filter();

        if self.list_state.selected().is_none() && !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn sort(&mut self) {
        match self.sort_by {
            SortBy::Cpu => self.processes.sort_by(|a, b| {
                b.cpu
                    .partial_cmp(&a.cpu)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortBy::Memory => self.processes.sort_by(|a, b| {
                b.mem_mb
                    .partial_cmp(&a.mem_mb)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortBy::Pid => self.processes.sort_by_key(|p| p.pid),
            SortBy::Name => self
                .processes
                .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        }
    }

    fn apply_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered = self
            .processes
            .iter()
            .enumerate()
            .filter(|(_, p)| q.is_empty() || p.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
    }

    pub fn visible_processes(&self) -> Vec<&ProcessEntry> {
        self.filtered
            .iter()
            .filter_map(|&i| self.processes.get(i))
            .collect()
    }

    pub fn selected_process(&self) -> Option<&ProcessEntry> {
        let idx = self.list_state.selected()?;
        let proc_idx = *self.filtered.get(idx)?;
        self.processes.get(proc_idx)
    }

    pub fn kill_selected(&mut self) -> bool {
        if let Some(proc) = self.selected_process() {
            let pid = sysinfo::Pid::from_u32(proc.pid);
            if let Some(p) = self.sys.process(pid) {
                p.kill();
                return true;
            }
        }
        false
    }

    pub fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    pub fn move_down(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i + 1 < self.filtered.len() {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    pub fn set_sort(&mut self, sort: SortBy) {
        self.sort_by = sort;
        self.sort();
        self.apply_filter();
    }

    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.apply_filter();
        self.list_state.select(if self.filtered.is_empty() {
            None
        } else {
            Some(0)
        });
    }

    pub fn filter_pop(&mut self) {
        self.filter.pop();
        self.apply_filter();
    }
}

impl Default for ProcessViewer {
    fn default() -> Self {
        Self::new()
    }
}
