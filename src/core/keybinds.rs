use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    // Navigation
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    FocusNext,
    FocusPrev,
    SwitchWindow(u8),
    FocusPrevWindow,

    // Tabs
    NewTab,
    CloseTab,
    TabNext,
    TabPrev,
    SwitchTab(u8),

    // File operations
    Open,
    Save,
    SaveAs,
    Copy,
    Cut,
    Paste,
    Delete,
    Rename,
    NewFile,
    NewDir,
    Refresh,

    // Edit
    Undo,
    Redo,
    SelectAll,
    Find,

    // App
    CommandPalette,
    GlobalMenu,
    Help,
    Quit,
    Escape,
    Confirm,

    // Resize
    ResizeSplitLeft,
    ResizeSplitRight,

    // Module-specific (passed through)
    PlayPause,
    NextTrack,
    PrevTrack,
    VolumeUp,
    VolumeDown,
    SeekForward,
    SeekBack,
    ToggleHidden,
    ToggleSidePanel,
    KillProcess,
    StageFile,
    CommitGit,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBind {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBind {
    #[allow(dead_code)]
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::NONE,
        }
    }

    pub fn ctrl(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::CONTROL,
        }
    }

    pub fn alt(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::ALT,
        }
    }

    pub fn shift(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::SHIFT,
        }
    }
}

pub struct KeybindEngine {
    map: HashMap<KeyBind, Action>,
}

impl KeybindEngine {
    pub fn default_gui() -> Self {
        let mut map = HashMap::new();

        // Navigation
        map.insert(KeyBind::plain(KeyCode::Up), Action::Up);
        map.insert(KeyBind::plain(KeyCode::Down), Action::Down);
        map.insert(KeyBind::plain(KeyCode::Left), Action::Left);
        map.insert(KeyBind::plain(KeyCode::Right), Action::Right);
        map.insert(KeyBind::plain(KeyCode::PageUp), Action::PageUp);
        map.insert(KeyBind::plain(KeyCode::PageDown), Action::PageDown);
        map.insert(KeyBind::plain(KeyCode::Home), Action::Home);
        map.insert(KeyBind::plain(KeyCode::End), Action::End);
        map.insert(KeyBind::plain(KeyCode::Tab), Action::FocusNext);
        map.insert(KeyBind::shift(KeyCode::BackTab), Action::FocusPrev);
        map.insert(KeyBind::alt(KeyCode::Tab), Action::FocusPrevWindow);

        // Alt+1–9: switch window
        for i in 1u8..=9 {
            let c = KeyCode::Char(char::from_digit(i as u32, 10).unwrap());
            map.insert(KeyBind::alt(c), Action::SwitchWindow(i));
        }

        // Tabs
        map.insert(KeyBind::ctrl(KeyCode::Char('t')), Action::NewTab);
        map.insert(KeyBind::ctrl(KeyCode::Char('w')), Action::CloseTab);
        // Ctrl+- closes the active tab.
        map.insert(KeyBind::ctrl(KeyCode::Char('-')), Action::CloseTab);
        for i in 1u8..=9 {
            let c = KeyCode::Char(char::from_digit(i as u32, 10).unwrap());
            map.insert(KeyBind::ctrl(c), Action::SwitchTab(i));
        }
        // Ctrl+0 → tab 10
        map.insert(KeyBind::ctrl(KeyCode::Char('0')), Action::SwitchTab(10));

        // File
        map.insert(KeyBind::ctrl(KeyCode::Char('s')), Action::Save);
        map.insert(KeyBind::ctrl(KeyCode::Char('c')), Action::Copy);
        map.insert(KeyBind::ctrl(KeyCode::Char('x')), Action::Cut);
        map.insert(KeyBind::ctrl(KeyCode::Char('v')), Action::Paste);
        map.insert(KeyBind::plain(KeyCode::Delete), Action::Delete);
        map.insert(KeyBind::plain(KeyCode::F(5)), Action::Refresh);
        map.insert(KeyBind::plain(KeyCode::Enter), Action::Confirm);

        // Edit
        map.insert(KeyBind::ctrl(KeyCode::Char('z')), Action::Undo);
        map.insert(KeyBind::ctrl(KeyCode::Char('y')), Action::Redo);
        map.insert(KeyBind::ctrl(KeyCode::Char('a')), Action::SelectAll);
        map.insert(KeyBind::ctrl(KeyCode::Char('f')), Action::Find);

        // App
        map.insert(KeyBind::ctrl(KeyCode::Char('p')), Action::CommandPalette);
        map.insert(KeyBind::plain(KeyCode::F(2)), Action::GlobalMenu);
        map.insert(KeyBind::plain(KeyCode::F(1)), Action::Help);
        map.insert(KeyBind::plain(KeyCode::F(10)), Action::Quit);
        map.insert(KeyBind::plain(KeyCode::Esc), Action::Escape);

        // Split resize
        map.insert(KeyBind::ctrl(KeyCode::Left), Action::ResizeSplitLeft);
        map.insert(KeyBind::ctrl(KeyCode::Right), Action::ResizeSplitRight);

        // Media
        map.insert(KeyBind::plain(KeyCode::Char(' ')), Action::PlayPause);
        map.insert(KeyBind::plain(KeyCode::Char('n')), Action::NextTrack);
        map.insert(KeyBind::plain(KeyCode::Char('p')), Action::PrevTrack);
        map.insert(KeyBind::plain(KeyCode::Char('+')), Action::VolumeUp);
        map.insert(KeyBind::plain(KeyCode::Char('-')), Action::VolumeDown);

        // Toggle
        map.insert(KeyBind::plain(KeyCode::Char('h')), Action::ToggleHidden);
        map.insert(KeyBind::plain(KeyCode::Char('t')), Action::ToggleSidePanel);

        Self { map }
    }

    pub fn resolve(&self, event: &KeyEvent) -> Option<&Action> {
        let bind = KeyBind {
            code: event.code,
            modifiers: event.modifiers,
        };
        self.map.get(&bind)
    }
}
