#![allow(dead_code)]
use std::collections::HashMap;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum BusEvent {
    FileOpened(String),
    FileSaved(String),
    DirectoryChanged(String),
    ProcessKilled(u32),
    NotificationAdded(String),
    ThemeChanged(String),
    ModuleActivated(String),
    TerminalOutput(String),
    GitStatusRefreshed,
    SessionSaved,
}

pub struct EventBus {
    senders: HashMap<&'static str, broadcast::Sender<BusEvent>>,
}

impl EventBus {
    pub fn new() -> Self {
        let mut senders = HashMap::new();
        let (tx, _) = broadcast::channel(64);
        senders.insert("global", tx);
        Self { senders }
    }

    pub fn publish(&self, event: BusEvent) {
        if let Some(tx) = self.senders.get("global") {
            let _ = tx.send(event);
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.senders["global"].subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
