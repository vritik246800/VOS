use crate::core::state::AppMode;
use crate::ui::icons::{
    ICON_MODE_AUDIO, ICON_MODE_CALC, ICON_MODE_CALENDAR, ICON_MODE_CONFIG, ICON_MODE_CRON,
    ICON_MODE_DISK, ICON_MODE_DOCKER, ICON_MODE_EDITOR, ICON_MODE_FAVORITES, ICON_MODE_FILES,
    ICON_MODE_GIT, ICON_MODE_HELP, ICON_MODE_IMAGE, ICON_MODE_LOGS, ICON_MODE_MAN, ICON_MODE_MENU,
    ICON_MODE_NETWORK, ICON_MODE_NOTES, ICON_MODE_PACKAGES, ICON_MODE_PROCESS, ICON_MODE_SERVICES,
    ICON_MODE_SFTP, ICON_MODE_SSH, ICON_MODE_TERMINAL, ICON_MODE_VIDEO, ICON_MODE_WEATHER,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Tabs},
};

// ── Data model ────────────────────────────────────────────────────────────────

/// One bordered sub-panel inside a tab: free text lines and/or a key table.
pub struct HelpSection {
    pub title: &'static str,
    pub text: &'static [&'static str],
    pub keys: &'static [(&'static str, &'static str)],
}

pub struct HelpTab {
    pub title: &'static str,
    pub sections: &'static [HelpSection],
}

pub struct HelpTopic {
    pub icon: &'static str,
    pub name: &'static str,
    pub tabs: &'static [HelpTab],
}

pub struct HelpGroup {
    pub name: &'static str,
    pub topics: &'static [HelpTopic],
}

impl HelpSection {
    fn height(&self) -> u16 {
        let gap = if !self.text.is_empty() && !self.keys.is_empty() {
            1
        } else {
            0
        };
        2 + self.text.len() as u16 + self.keys.len() as u16 + gap
    }
}

// ── Content ───────────────────────────────────────────────────────────────────

pub const HELP: &[HelpGroup] = &[
    HelpGroup {
        name: "Getting started",
        topics: &[
            HelpTopic {
                icon: ICON_MODE_HELP,
                name: "Overview",
                tabs: &[
                    HelpTab {
                        title: "What is VOS",
                        sections: &[
                            HelpSection {
                                title: "VOS — Terminal Environment",
                                text: &[
                                    "A modular TUI environment inspired by Midnight",
                                    "Commander, Vim, Tmux and Hyprland.",
                                    "No Vim modes — shortcuts are always active.",
                                ],
                                keys: &[],
                            },
                            HelpSection {
                                title: "Finding things",
                                text: &[],
                                keys: &[
                                    ("F1", "this help"),
                                    ("F2", "main menu"),
                                    ("Ctrl+P", "command palette (fuzzy search)"),
                                    (":", "command mode"),
                                    ("hint bar", "per-mode keys, always on screen"),
                                ],
                            },
                        ],
                    },
                    HelpTab {
                        title: "Modules",
                        sections: &[
                            HelpSection {
                                title: "Core",
                                text: &[],
                                keys: &[
                                    ("Menu", "home screen"),
                                    ("File Manager", "file explorer"),
                                    ("Editor", "text editor"),
                                    ("Terminal", "integrated shell"),
                                    ("Monitor", "system dashboard + `d` processes"),
                                    ("Git", "full git panel"),
                                    ("Favorites", "bookmarked paths"),
                                    ("Config", "settings editor"),
                                ],
                            },
                            HelpSection {
                                title: "Media · System · Admin · Apps",
                                text: &[],
                                keys: &[
                                    ("Media", "audio, video, image, PDF"),
                                    ("System", "logs, services, network, disk, calc"),
                                    ("Admin", "packages, SSH, SFTP, Docker, cron, man"),
                                    ("Apps", "notes, weather, calendar"),
                                ],
                            },
                        ],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_MENU,
                name: "Tabs & panels",
                tabs: &[
                    HelpTab {
                        title: "Tabs",
                        sections: &[HelpSection {
                            title: "Each tab is its own activity",
                            text: &[
                                "Every tab keeps its own module + state — several file",
                                "managers at different paths, a Monitor, an editor, …",
                            ],
                            keys: &[
                                ("Ctrl+T", "new tab (opens the menu, max 10)"),
                                ("Ctrl+-", "close active tab"),
                                ("Ctrl+W", "close active tab (alias)"),
                                ("Ctrl+1..9, 0", "go to tab N (0 = tab 10)"),
                                ("Alt+1..9", "switch window"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "Side panel & splits",
                        sections: &[
                            HelpSection {
                                title: "Side panel (animated slide-in)",
                                text: &[],
                                keys: &[
                                    ("Ctrl+G", "git side panel"),
                                    ("Ctrl+\\", "side terminal at the current path"),
                                    ("m", "mini music panel (while audio plays)"),
                                    ("Tab / Shift+Tab", "move focus"),
                                ],
                            },
                            HelpSection {
                                title: "Splits",
                                text: &[],
                                keys: &[
                                    (":split h", "split horizontally"),
                                    (":split v", "split vertically"),
                                    ("Ctrl+← / Ctrl+→", "resize split"),
                                ],
                            },
                        ],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_TERMINAL,
                name: "Commands",
                tabs: &[
                    HelpTab {
                        title: "General",
                        sections: &[HelpSection {
                            title: "':' commands",
                            text: &["Press ':' then type. Esc cancels."],
                            keys: &[
                                ("q · quit · wq", "quit"),
                                ("w · save", "save"),
                                ("open <path>", "open a file"),
                                ("split h|v", "split the panel"),
                                ("theme [name]", "theme switcher · set theme"),
                                ("help", "open this help"),
                                ("plugin list", "list loaded plugins"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "Modules",
                        sections: &[HelpSection {
                            title: "Open a module",
                            text: &[],
                            keys: &[
                                ("git [status|add <f>|commit <msg>|pull|push]", "git"),
                                ("terminal · term", "integrated terminal"),
                                ("ps · proc · sys · sysmon · monitor", "monitor"),
                                ("fav · favorites", "favorites"),
                                ("notes", "notes"),
                                ("weather · wttr", "weather"),
                                ("calendar · cal", "calendar"),
                                ("ssh", "ssh manager"),
                            ],
                        }],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_CONFIG,
                name: "Shortcuts",
                tabs: &[
                    HelpTab {
                        title: "Global",
                        sections: &[
                            HelpSection {
                                title: "Function keys",
                                text: &[],
                                keys: &[
                                    ("F1", "help"),
                                    ("F2", "menu"),
                                    ("F5", "refresh"),
                                    ("F10", "quit"),
                                    ("Esc", "back / close"),
                                ],
                            },
                            HelpSection {
                                title: "Editing",
                                text: &[],
                                keys: &[
                                    ("Ctrl+S", "save"),
                                    ("Ctrl+Z / Ctrl+Y", "undo / redo"),
                                    ("Ctrl+C / Ctrl+X / Ctrl+V", "copy / cut / paste"),
                                    ("Ctrl+F", "find"),
                                ],
                            },
                            HelpSection {
                                title: "Tools",
                                text: &[],
                                keys: &[
                                    ("Ctrl+P", "command palette"),
                                    ("Ctrl+A", "calculator (select-all in editor/terminal)"),
                                    ("Ctrl+G / Ctrl+\\", "git panel / side terminal"),
                                ],
                            },
                        ],
                    },
                    HelpTab {
                        title: "Players",
                        sections: &[
                            HelpSection {
                                title: "Music player",
                                text: &[],
                                keys: &[
                                    ("Space", "play / pause"),
                                    ("n / p", "next / previous track"),
                                    ("+ / -", "volume"),
                                    ("← / →", "seek 10s"),
                                    ("Esc", "close (keeps playing)"),
                                ],
                            },
                            HelpSection {
                                title: "Video player",
                                text: &[],
                                keys: &[
                                    ("Space", "play / pause"),
                                    ("← / →", "seek 5s"),
                                    ("Esc / q", "close"),
                                ],
                            },
                        ],
                    },
                ],
            },
        ],
    },
    HelpGroup {
        name: "Core",
        topics: &[
            HelpTopic {
                icon: ICON_MODE_FILES,
                name: "File Manager",
                tabs: &[
                    HelpTab {
                        title: "Keys",
                        sections: &[
                            HelpSection {
                                title: "Navigation",
                                text: &[],
                                keys: &[
                                    ("↑ / ↓", "move selection"),
                                    ("Enter", "enter folder / open file"),
                                    ("Backspace", "parent folder"),
                                    ("Tab", "switch focus between panels"),
                                    ("/", "search"),
                                    ("h", "show / hide hidden files"),
                                    ("Esc", "back to menu"),
                                ],
                            },
                            HelpSection {
                                title: "File operations",
                                text: &[],
                                keys: &[
                                    ("c / x / p", "copy / cut / paste"),
                                    ("d · Del", "delete"),
                                    ("r", "rename"),
                                    ("b", "bookmark"),
                                    ("B", "open favorites"),
                                ],
                            },
                        ],
                    },
                    HelpTab {
                        title: "Auto detection",
                        sections: &[HelpSection {
                            title: "Opening a file picks the module",
                            text: &[],
                            keys: &[
                                (".mp3 .flac .wav .ogg .m4a", "music player"),
                                (".mp4 .mkv .avi .mov .webm", "video player"),
                                (".png .jpg .gif .bmp .webp", "image viewer"),
                                (".pdf", "PDF viewer"),
                                (".git in folder", "git side panel"),
                            ],
                        }],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_EDITOR,
                name: "Editor",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[
                        HelpSection {
                            title: "Editing",
                            text: &[],
                            keys: &[
                                ("↑ ↓ ← →", "move cursor"),
                                ("Backspace", "delete previous character"),
                                ("Enter", "new line"),
                                ("Ctrl+S", "save"),
                                ("Ctrl+Z / Ctrl+Y", "undo / redo"),
                                ("Esc", "back to file manager"),
                            ],
                        },
                        HelpSection {
                            title: "Commands",
                            text: &[],
                            keys: &[
                                (":w", "save"),
                                (":wq", "save and quit"),
                                (":q", "quit without saving"),
                                (":open <path>", "open file by path"),
                                (":split h|v", "split the panel"),
                            ],
                        },
                    ],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_TERMINAL,
                name: "Terminal",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[
                        HelpSection {
                            title: "Shell",
                            text: &["Async shell, persistent cd between commands."],
                            keys: &[
                                ("type + Enter", "run command"),
                                ("↑ / ↓", "command history"),
                                ("clear", "clear screen (scrollback kept)"),
                                ("cd <dir>", "change directory (persists)"),
                                (":", "VOS command mode (empty input)"),
                                ("Esc", "exit terminal"),
                            ],
                        },
                        HelpSection {
                            title: "Scroll mode",
                            text: &[],
                            keys: &[
                                ("Tab", "enter scroll mode"),
                                ("↑ / ↓", "scroll output"),
                                ("Tab · Esc", "leave scroll mode"),
                            ],
                        },
                        HelpSection {
                            title: "Side terminal (Ctrl+\\)",
                            text: &[
                                "Opens at the current file-manager path and uses",
                                "the same engine. Ctrl+\\ again closes it.",
                            ],
                            keys: &[],
                        },
                    ],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_PROCESS,
                name: "Monitor",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[
                        HelpSection {
                            title: "Dashboard (default view)",
                            text: &[
                                "CPU with a per-core grid, memory, swap, networking",
                                "(down/upload per second + per-interface rows) and",
                                "storage split into main, USB and network volumes.",
                            ],
                            keys: &[("d", "swap to the process list"), ("Esc", "back to menu")],
                        },
                        HelpSection {
                            title: "Processes",
                            text: &["Live process list, sortable and filterable."],
                            keys: &[
                                ("↑ / ↓", "navigate"),
                                ("k", "kill selected process"),
                                ("1-4", "sort column"),
                                ("/", "filter"),
                                ("F5", "refresh"),
                                ("d · Esc", "back to dashboard"),
                            ],
                        },
                    ],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_GIT,
                name: "Git",
                tabs: &[
                    HelpTab {
                        title: "Panel",
                        sections: &[
                            HelpSection {
                                title: "Working tree",
                                text: &[],
                                keys: &[
                                    ("a", "stage all (git add .)"),
                                    ("u", "unstage"),
                                    ("c", "commit (inline dialog)"),
                                    ("p / P", "pull / push"),
                                    ("i", "create .gitignore"),
                                    ("F5 · r", "refresh status"),
                                ],
                            },
                            HelpSection {
                                title: "Browsing",
                                text: &[],
                                keys: &[
                                    ("↑ / ↓", "navigate file list"),
                                    ("l", "log with graph"),
                                    ("b", "branches"),
                                    ("Tab", "switch pane"),
                                    ("Enter", "open selected repo"),
                                    ("Esc", "back (detail → list → menu)"),
                                ],
                            },
                        ],
                    },
                    HelpTab {
                        title: "Commands",
                        sections: &[HelpSection {
                            title: "':' git",
                            text: &["The panel opens automatically in a .git folder."],
                            keys: &[
                                (":git status", "status"),
                                (":git add <file>", "stage a file"),
                                (":git commit <msg>", "commit"),
                                (":git pull", "pull"),
                                (":git push", "push"),
                            ],
                        }],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_FAVORITES,
                name: "Favorites & themes",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[
                        HelpSection {
                            title: "Favorites",
                            text: &["Bookmarked files and folders, stored in app.db."],
                            keys: &[
                                ("↑ / ↓", "select"),
                                ("Enter", "open"),
                                ("b (file manager)", "bookmark current entry"),
                                ("Esc", "back"),
                            ],
                        },
                        HelpSection {
                            title: "Themes & config",
                            text: &["Config writes config/config.toml."],
                            keys: &[
                                (":theme", "open the theme switcher"),
                                (":theme <name>", "dark · light · neon · solarized · gruvbox"),
                                ("↑ / ↓ then Enter", "apply a theme"),
                                ("↑ / ↓ (config)", "select field"),
                                ("← / → · Enter", "change value"),
                                ("Esc", "save + close"),
                            ],
                        },
                    ],
                }],
            },
        ],
    },
    HelpGroup {
        name: "Media",
        topics: &[
            HelpTopic {
                icon: ICON_MODE_AUDIO,
                name: "Audio Player",
                tabs: &[
                    HelpTab {
                        title: "Library",
                        sections: &[HelpSection {
                            title: "Artists → Albums → Tracks",
                            text: &["Scanned from settings.music_dir, indexed in app.db."],
                            keys: &[
                                ("↑ / ↓", "navigate"),
                                ("Enter", "drill in / play track"),
                                ("Backspace", "one level up"),
                                ("Tab", "focus library ↔ queue"),
                                ("F5", "scan for new tracks"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "Queue",
                        sections: &[HelpSection {
                            title: "Queue management",
                            text: &[],
                            keys: &[
                                ("a", "add selected track to queue"),
                                ("d", "remove selected track from queue"),
                                ("Enter", "jump to track in queue"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "Playback",
                        sections: &[
                            HelpSection {
                                title: "Always active",
                                text: &[],
                                keys: &[
                                    ("Space", "play / pause"),
                                    ("n / p", "next / previous track"),
                                    ("+ · = / -", "volume up / down"),
                                    ("← / →", "seek 10s"),
                                ],
                            },
                            HelpSection {
                                title: "Window",
                                text: &["Audio keeps playing in every other module."],
                                keys: &[
                                    ("m", "minimize to the mini side panel"),
                                    ("m (global)", "toggle the mini panel"),
                                    ("Esc", "background (keeps playing)"),
                                ],
                            },
                        ],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_VIDEO,
                name: "Video Player",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[HelpSection {
                        title: "Playback",
                        text: &[
                            "ffmpeg decodes with an 8-frame prefetch thread.",
                            "Kitty terminals get native resolution, others",
                            "fall back to ANSI half-blocks.",
                        ],
                        keys: &[
                            ("Space", "play / pause"),
                            ("← / →", "seek 5s"),
                            ("Esc · q", "close"),
                        ],
                    }],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_IMAGE,
                name: "Image & PDF",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[
                        HelpSection {
                            title: "Image viewer",
                            text: &["PNG/JPEG/GIF/BMP/WebP/TIFF, Kitty or half-block."],
                            keys: &[("Esc · q", "close"), ("i (notes)", "open embedded image")],
                        },
                        HelpSection {
                            title: "PDF viewer",
                            text: &["Text extraction via pdftotext (poppler)."],
                            keys: &[
                                ("↑ / ↓", "scroll"),
                                ("PgUp / PgDn", "page"),
                                ("Esc", "close"),
                            ],
                        },
                    ],
                }],
            },
        ],
    },
    HelpGroup {
        name: "System",
        topics: &[
            HelpTopic {
                icon: ICON_MODE_LOGS,
                name: "Log Viewer",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[
                        HelpSection {
                            title: "Scrolling",
                            text: &["Live tail of the app log stored in app.db."],
                            keys: &[
                                ("↑ / ↓", "scroll"),
                                ("PgUp / PgDn", "page"),
                                ("End", "jump to the newest line"),
                                ("f", "follow mode"),
                                ("Esc", "back"),
                            ],
                        },
                        HelpSection {
                            title: "Level filters",
                            text: &[],
                            keys: &[
                                ("e", "errors"),
                                ("w", "warnings"),
                                ("i", "info"),
                                ("0", "clear the filter"),
                            ],
                        },
                    ],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_SERVICES,
                name: "Services",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[HelpSection {
                        title: "systemd units",
                        text: &["Non-systemd hosts show a notice instead."],
                        keys: &[
                            ("↑ / ↓", "select"),
                            ("s", "start"),
                            ("t", "stop"),
                            ("r", "restart"),
                            ("e", "enable"),
                            ("F5", "refresh"),
                            ("Esc", "back"),
                        ],
                    }],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_NETWORK,
                name: "Network",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[HelpSection {
                        title: "Interfaces",
                        text: &["Parses 'ip -o addr', falls back to ifconfig."],
                        keys: &[
                            ("↑ / ↓", "select interface"),
                            ("F5", "refresh + ping"),
                            ("Esc", "back"),
                        ],
                    }],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_DISK,
                name: "Disk Usage",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[HelpSection {
                        title: "ncdu-style analyser",
                        text: &["Recursive scan on a background thread."],
                        keys: &[
                            ("↑ / ↓", "navigate"),
                            ("Enter", "enter folder"),
                            ("Backspace · ←", "parent folder"),
                            ("d · Del", "delete selected"),
                            ("F5", "rescan"),
                            ("Esc", "back"),
                        ],
                    }],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_CALC,
                name: "Calculator",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[HelpSection {
                        title: "Bottom sheet (Ctrl+A)",
                        text: &["Own parser: + - * / % ^, parentheses, unary minus."],
                        keys: &[
                            ("type", "build the expression"),
                            ("Enter", "evaluate (result goes to history)"),
                            ("Backspace", "delete a character"),
                            ("Esc", "close the sheet"),
                        ],
                    }],
                }],
            },
        ],
    },
    HelpGroup {
        name: "Admin & remote",
        topics: &[
            HelpTopic {
                icon: ICON_MODE_PACKAGES,
                name: "Packages",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[HelpSection {
                        title: "brew / apt / dnf / pacman",
                        text: &["The available backend is detected at startup."],
                        keys: &[
                            ("↑ / ↓", "select"),
                            ("Tab", "installed ↔ search"),
                            ("type", "search query"),
                            ("Backspace", "edit the query"),
                            ("F5", "reload installed"),
                            ("Esc", "back"),
                        ],
                    }],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_SSH,
                name: "SSH Manager",
                tabs: &[
                    HelpTab {
                        title: "Session",
                        sections: &[HelpSection {
                            title: "Connect",
                            text: &[
                                "Hosts parsed from ~/.ssh/config. Identity files are",
                                "never read — only their path is shown.",
                            ],
                            keys: &[
                                ("↑ / ↓", "select a host"),
                                ("Enter", "interactive session (TUI suspends)"),
                                ("t", "test connectivity (BatchMode, 3s)"),
                                ("F5", "reload config + groups + history"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "History",
                        sections: &[HelpSection {
                            title: "Past connections",
                            text: &[],
                            keys: &[
                                ("h", "open the history popup"),
                                ("↑ / ↓ · Enter", "select · reconnect"),
                                ("[ / ]", "browse this run's session tabs"),
                                ("x", "remove a session tab"),
                                ("Esc", "close the popup"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "Groups",
                        sections: &[HelpSection {
                            title: "Host groups",
                            text: &[
                                "Label hosts with '# group: <name>' in ~/.ssh/config,",
                                "or assign them from here.",
                            ],
                            keys: &[
                                ("g", "collapse / expand the group"),
                                ("n", "new group"),
                                ("a", "assign the selected host to a group"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "SFTP & tunnels",
                        sections: &[
                            HelpSection {
                                title: "SFTP",
                                text: &[],
                                keys: &[
                                    ("s", "open SFTP for the selected host"),
                                    ("Tab", "switch pane"),
                                    ("Enter · Backspace", "open dir · up"),
                                    ("g / p", "download / upload"),
                                    ("F5 · Esc", "refresh · close"),
                                ],
                            },
                            HelpSection {
                                title: "Tunnels (local port-forward)",
                                text: &[],
                                keys: &[
                                    ("T", "open the tunnels popup"),
                                    ("c", "create local:host:remote"),
                                    ("d", "kill the selected tunnel"),
                                    ("Esc · T", "close"),
                                ],
                            },
                        ],
                    },
                    HelpTab {
                        title: "Hosts",
                        sections: &[
                            HelpSection {
                                title: "Add / edit / remove",
                                text: &[
                                    "~/.ssh/config is backed up to data/ssh_config.bak",
                                    "before every write. Passwords are never saved —",
                                    "ssh always prompts for them itself.",
                                ],
                                keys: &[
                                    ("Tab", "new connection form"),
                                    ("e", "edit the selected host"),
                                    ("d", "remove the host (asks to confirm)"),
                                ],
                            },
                            HelpSection {
                                title: "In the form",
                                text: &[],
                                keys: &[
                                    ("Tab / Shift+Tab", "switch field"),
                                    ("Enter", "connect, then offer to save"),
                                    ("Esc", "cancel"),
                                ],
                            },
                        ],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_SFTP,
                name: "SFTP",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[HelpSection {
                        title: "Two-pane transfer",
                        text: &["Runs the sftp binary, all I/O on background threads."],
                        keys: &[
                            ("Tab", "local ↔ remote pane"),
                            ("↑ / ↓", "navigate"),
                            ("Enter · Backspace", "open dir · up"),
                            ("g / p", "download / upload"),
                            ("F5 · Esc", "refresh · close"),
                        ],
                    }],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_DOCKER,
                name: "Docker",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[HelpSection {
                        title: "Containers · images · volumes",
                        text: &["Falls back gracefully when the daemon is down."],
                        keys: &[
                            ("Tab", "switch section"),
                            ("↑ / ↓", "select"),
                            ("s / S", "start / stop"),
                            ("r", "restart"),
                            ("F5", "refresh"),
                            ("Esc", "back"),
                        ],
                    }],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_CRON,
                name: "Cron",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[
                        HelpSection {
                            title: "Job list",
                            text: &["Nothing is written until you press w."],
                            keys: &[
                                ("↑ / ↓", "select job"),
                                ("a", "add a job"),
                                ("e · Enter", "edit the job"),
                                ("d · Del", "delete the job"),
                                ("w", "write the crontab"),
                                ("Esc", "back"),
                            ],
                        },
                        HelpSection {
                            title: "Inline editor",
                            text: &[],
                            keys: &[
                                ("Tab / Shift+Tab", "switch field"),
                                ("Enter", "confirm"),
                                ("Esc", "cancel"),
                            ],
                        },
                    ],
                }],
            },
            HelpTopic {
                icon: ICON_MODE_MAN,
                name: "Man Viewer",
                tabs: &[HelpTab {
                    title: "Keys",
                    sections: &[
                        HelpSection {
                            title: "Reading",
                            text: &["Overstrike-aware formatting (bold / underline)."],
                            keys: &[
                                ("↑ / ↓", "scroll"),
                                ("PgUp / PgDn", "page"),
                                ("Home / End", "top / bottom"),
                                ("Esc · q", "close"),
                            ],
                        },
                        HelpSection {
                            title: "Search",
                            text: &[],
                            keys: &[("/", "start a search"), ("n / N", "next / previous match")],
                        },
                    ],
                }],
            },
        ],
    },
    HelpGroup {
        name: "Apps",
        topics: &[
            HelpTopic {
                icon: ICON_MODE_NOTES,
                name: "Notes",
                tabs: &[
                    HelpTab {
                        title: "Notes",
                        sections: &[HelpSection {
                            title: "Markdown notes",
                            text: &[
                                "Stored in settings.notes_dir (default ~/Notes) and",
                                "indexed in app.db for fuzzy search.",
                            ],
                            keys: &[
                                ("↑ / ↓", "select note"),
                                ("PgUp / PgDn", "scroll the preview"),
                                ("Enter", "open in the editor"),
                                ("n", "new note (asks for a title)"),
                                ("i", "open the first embedded image"),
                                ("F5", "rescan the folder"),
                                ("Esc", "back to menu"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "Search & open",
                        sections: &[HelpSection {
                            title: "Finding a note",
                            text: &[],
                            keys: &[
                                ("Ctrl+P + title", "notes show as 'note: <title>'"),
                                (":notes", "open the Notes module"),
                            ],
                        }],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_WEATHER,
                name: "Weather",
                tabs: &[
                    HelpTab {
                        title: "Dashboard",
                        sections: &[HelpSection {
                            title: "Cities",
                            text: &[
                                "open-meteo.com, no API key. City list in app.db,",
                                "each city cached for 15 minutes.",
                            ],
                            keys: &[
                                ("↑ / ↓", "select a city"),
                                ("F5", "refresh (uses the cache)"),
                                ("r", "force refresh (ignores the cache)"),
                                ("Esc", "back to menu"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "Cities & units",
                        sections: &[
                            HelpSection {
                                title: "Add / remove",
                                text: &[
                                    "'a' opens a search overlay: type ≥3 letters to",
                                    "list live matches, ↑/↓ + Enter to add.",
                                ],
                                keys: &[("a", "add a city"), ("d · Del", "remove the city")],
                            },
                            HelpSection {
                                title: "Units",
                                text: &[],
                                keys: &[("u · c · f", "toggle °C ↔ °F (saved in settings)")],
                            },
                        ],
                    },
                ],
            },
            HelpTopic {
                icon: ICON_MODE_CALENDAR,
                name: "Calendar",
                tabs: &[
                    HelpTab {
                        title: "Grid",
                        sections: &[HelpSection {
                            title: "Month navigation",
                            text: &["Events are shown inline in each day cell."],
                            keys: &[
                                ("← / →", "previous / next month"),
                                ("h / l", "move selected day ∓1 day"),
                                ("↑ / ↓", "move selected day ∓1 week"),
                                ("t", "jump to today"),
                                ("Esc", "back to menu"),
                            ],
                        }],
                    },
                    HelpTab {
                        title: "Tasks",
                        sections: &[HelpSection {
                            title: "Inline editor (right pane)",
                            text: &["Tasks are stored in app.db (table 'tasks')."],
                            keys: &[
                                ("Tab · a", "jump to the editor"),
                                ("type", "write the task title"),
                                ("Enter", "save (stays editing for the next)"),
                                ("↑ / ↓", "select an existing task"),
                                ("Enter (empty)", "toggle done"),
                                ("Del", "delete the task"),
                                ("Tab · Esc", "save pending text, back to the grid"),
                            ],
                        }],
                    },
                ],
            },
        ],
    },
];

/// Flattened topic list — the left panel selection index refers to this order.
pub fn topics() -> Vec<&'static HelpTopic> {
    HELP.iter().flat_map(|g| g.topics.iter()).collect()
}

pub fn topic_count() -> usize {
    HELP.iter().map(|g| g.topics.len()).sum()
}

pub fn tab_count(topic: usize) -> usize {
    topics().get(topic).map(|t| t.tabs.len()).unwrap_or(1)
}

pub fn section_count(topic: usize, tab: usize) -> usize {
    topics()
        .get(topic)
        .and_then(|t| t.tabs.get(tab))
        .map(|t| t.sections.len())
        .unwrap_or(0)
}

// ── Key-hint bar (unchanged) ──────────────────────────────────────────────────

/// Returns (key, description) hint pairs for the given mode.
pub fn keymap_for(mode: &AppMode) -> Vec<(&'static str, &'static str)> {
    match mode {
        AppMode::FileManager => vec![
            ("↑/↓", "navigate"),
            ("Enter", "open"),
            ("Bksp", "up"),
            ("b", "bookmark"),
            ("B", "favorites"),
            ("c", "copy"),
            ("x", "cut"),
            ("p", "paste"),
            ("d/Del", "delete"),
            ("r", "rename"),
            ("h", "hidden"),
            ("/", "search"),
            ("Esc", "menu"),
        ],
        AppMode::Git => vec![
            ("a", "stage"),
            ("u", "unstage"),
            ("c", "commit"),
            ("b", "branches"),
            ("p", "pull"),
            ("P", "push"),
            ("Tab", "pane"),
            ("F5", "refresh"),
            ("Esc", "close"),
        ],
        AppMode::ProcessViewer => vec![
            ("↑/↓", "navigate"),
            ("k", "kill"),
            ("1-4", "sort"),
            ("/", "filter"),
            ("F5", "refresh"),
            ("Esc", "menu"),
        ],
        AppMode::AudioPlayer => vec![
            ("Space", "play/pause"),
            ("Tab", "switch focus"),
            ("↑/↓", "navigate"),
            ("Enter", "drill-down/play"),
            ("Bksp", "go back"),
            ("a", "add to queue"),
            ("d", "remove from queue"),
            ("n", "next track"),
            ("p", "prev track"),
            ("+/-", "volume"),
            ("←/→", "seek 10s"),
            ("F5", "scan library"),
            ("m", "minimize to mini"),
            ("Esc", "background"),
        ],
        AppMode::VideoPlayer => vec![
            ("Space", "play/pause"),
            ("←/→", "seek 5s"),
            ("Esc/q", "close"),
        ],
        AppMode::Editor => vec![
            ("Ctrl+S", "save"),
            ("Ctrl+Z", "undo"),
            ("Ctrl+Y", "redo"),
            (":", "command"),
            ("Esc", "files"),
        ],
        AppMode::Config => vec![
            ("↑/↓", "field"),
            ("←/→/Enter", "toggle"),
            ("Esc", "save+close"),
        ],
        AppMode::Help => vec![
            ("↑/↓", "topic"),
            ("←/→/Tab", "tab"),
            ("PgUp/PgDn", "scroll"),
            ("Esc", "close"),
        ],
        AppMode::Notes => vec![
            ("↑/↓", "select note"),
            ("PgUp/PgDn", "scroll preview"),
            ("Enter", "edit"),
            ("n", "new note"),
            ("i", "open image"),
            ("Ctrl+P", "search notes"),
            ("F5", "rescan"),
            ("Esc", "menu"),
        ],
        AppMode::Weather => vec![
            ("↑/↓", "city"),
            ("a", "add city"),
            ("d", "remove"),
            ("u", "°C/°F"),
            ("F5", "refresh"),
            ("r", "force"),
            ("Esc", "menu"),
        ],
        AppMode::Calendar => vec![
            ("←/→", "month"),
            ("h/l", "±1 day"),
            ("↑/↓", "±1 week"),
            ("t", "today"),
            ("Tab/a", "add task (inline)"),
            ("Enter", "save task"),
            ("Del", "remove"),
            ("Esc", "menu"),
        ],
        _ => vec![],
    }
}

/// Renders a 1-line key-hints bar into the given area.
/// Format: `[key] action  [key] action ...` — keys in bold yellow, descriptions in dim.
pub fn render_key_hints(f: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    if hints.is_empty() || area.height == 0 {
        return;
    }

    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(
            format!("[{key}]"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {desc}"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        ));
    }

    let line = Line::from(spans);
    let para = Paragraph::new(line).style(Style::default().bg(Color::Reset));
    f.render_widget(para, area);
}

// ── Render ────────────────────────────────────────────────────────────────────

/// Help screen: grouped topic list on the left, tabbed sub-panels on the right.
/// `scroll` is the index of the first visible section inside the active tab.
pub fn render_help_panel(f: &mut Frame, area: Rect, topic: usize, tab: usize, scroll: usize) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(0)])
        .split(area);

    render_topic_list(f, cols[0], topic);

    let topics = topics();
    let Some(t) = topics.get(topic) else { return };

    let (tabs_area, body) = if t.tabs.len() > 1 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(cols[1]);
        (Some(rows[0]), rows[1])
    } else {
        (None, cols[1])
    };

    if let Some(rect) = tabs_area {
        let titles: Vec<Line> = t
            .tabs
            .iter()
            .map(|tb| Line::from(Span::styled(tb.title, Style::default().fg(Color::White))))
            .collect();
        let widget = Tabs::new(titles)
            .select(tab.min(t.tabs.len() - 1))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(format!(" {} {} ", t.icon, t.name))
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("│");
        f.render_widget(widget, rect);
    }

    let Some(active) = t.tabs.get(tab.min(t.tabs.len().saturating_sub(1))) else {
        return;
    };
    render_sections(f, body, t, active, scroll, tabs_area.is_none());
}

fn render_topic_list(f: &mut Frame, area: Rect, selected: usize) {
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_row = 0usize;
    let mut topic_idx = 0usize;

    for group in HELP {
        items.push(ListItem::new(Line::from(Span::styled(
            format!(" {} ", group.name.to_uppercase()),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ))));
        for topic in group.topics {
            if topic_idx == selected {
                selected_row = items.len();
            }
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {} ", topic.icon),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(topic.name, Style::default().fg(Color::White)),
            ])));
            topic_idx += 1;
        }
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Help — ↑/↓ topic ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(selected_row));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_sections(
    f: &mut Frame,
    area: Rect,
    topic: &HelpTopic,
    tab: &HelpTab,
    scroll: usize,
    show_title: bool,
) {
    if area.height == 0 {
        return;
    }
    let first = scroll.min(tab.sections.len().saturating_sub(1));
    let mut y = area.y;

    for (i, section) in tab.sections.iter().enumerate().skip(first) {
        if y >= area.y + area.height {
            break;
        }
        let h = section.height().min(area.y + area.height - y);
        let rect = Rect::new(area.x, y, area.width, h);

        let mut title = format!(" {} ", section.title);
        if show_title && i == first {
            title = format!(" {} {} · {} ", topic.icon, topic.name, section.title);
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(Span::styled(
                title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_style(Style::default().fg(Color::DarkGray));

        f.render_widget(Paragraph::new(section_lines(section)).block(block), rect);
        y += h;
    }

    // Scroll indicator when sections are left below.
    if y >= area.y + area.height && first + 1 < tab.sections.len() {
        let hint = Rect::new(area.x + 2, area.y + area.height - 1, area.width - 4, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " ↓ PgDn for more ",
                Style::default().fg(Color::Black).bg(Color::Yellow),
            ))),
            hint,
        );
    }
}

fn section_lines(section: &HelpSection) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = section
        .text
        .iter()
        .map(|t| Line::from(Span::styled(*t, Style::default().fg(Color::White))))
        .collect();

    if !section.text.is_empty() && !section.keys.is_empty() {
        lines.push(Line::from(""));
    }

    let width = section.keys.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, desc) in section.keys {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{key:<width$}", width = width),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  —  ", Style::default().fg(Color::DarkGray)),
            Span::styled(*desc, Style::default().fg(Color::White)),
        ]));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_index_is_consistent() {
        let topics = topics();
        assert_eq!(topics.len(), topic_count());
        for (i, topic) in topics.iter().enumerate() {
            assert!(!topic.tabs.is_empty(), "{} has no tab", topic.name);
            assert_eq!(tab_count(i), topic.tabs.len());
            for (t, tab) in topic.tabs.iter().enumerate() {
                assert!(!tab.sections.is_empty(), "{} has an empty tab", topic.name);
                assert_eq!(section_count(i, t), tab.sections.len());
                // A section must fit in a reasonable terminal, or it can never
                // be read: the scroll unit is one whole section.
                for s in tab.sections {
                    assert!(s.height() <= 24, "section '{}' too tall", s.title);
                }
            }
        }
    }
}
