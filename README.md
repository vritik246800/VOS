# VOS — Visual OS Shell

A modular TUI environment written in Rust (`ratatui` + `tokio`) that sits between the raw CLI and a
full GUI: a tabbed, window-managed shell where files, editor, terminal, git, system monitoring,
remote hosts and media all live in the same terminal window.

Inspired by Midnight Commander, Vim, tmux and Hyprland.

---

## Requirements

| Requirement | Needed for |
|---|---|
| Rust (edition 2024) | building |
| A Nerd Font in the terminal | all icons (JetBrains Mono Nerd Font, Hack Nerd Font, …) |
| `ffmpeg` + `ffprobe` in `$PATH` | video player (frame extraction + probing) |
| An audio output device | audio player (rodio) |
| `pdftotext` (poppler) | PDF viewer |
| Kitty-compatible terminal | *optional* — native-resolution image/video rendering |
| `docker`, `systemctl`, `crontab`, `ssh`/`sftp`, `brew`/`apt`/`dnf`/`pacman`, `man` | the matching module (each degrades gracefully when absent) |

## Build & run

```bash
cargo run              # run from the VOS project root
cargo build --release
cargo check            # fast type-check
cargo test
cargo fmt
cargo clippy
```

Always run from the project root — `config/config.toml` and `data/app.db` are resolved relative to
the current working directory. Both are created automatically on first run.

## Configuration

`config/config.toml` (auto-created, editable in-app through the Config panel):

| Key | Default | Meaning |
|---|---|---|
| `theme` | `"dark"` | `dark` · `light` · `neon` · `solarized` · `gruvbox` |
| `show_hidden` | `false` | show dotfiles in the file manager |
| `mouse_enabled` | `true` | mouse capture |
| `autosave` | `false` | editor autosave |
| `tab_width` | `4` | editor tab width |
| `notification_timeout` | `60` | notification lifetime (ticks) |
| `terminal_scrollback` | `2000` | integrated terminal scrollback lines |
| `music_dir` | `"~/Music"` | music library scan root |
| `notes_dir` | `"~/Notes"` | markdown notes folder |
| `latitude` / `longitude` / `location_name` | Lisbon | default weather location |
| `fahrenheit` | `false` | weather units (`u` toggles in-app) |

---

## Concepts

**Tabs** — each tab is a full activity with its own module and state (several file managers at
different paths, a monitor, an editor…). Max 10. `Ctrl+T` new, `Ctrl+-` / `Ctrl+W` close,
`Ctrl+1..9`/`0` jump.

**Windows & splits** — a window manager (`wm/`) with horizontal/vertical splits (`:split h|v`),
`Alt+1..9` to switch window, `Ctrl+←/→` to resize the split.

**Side panel** — animated slide-in panel that can hold Git, a terminal at the current path, the
system monitor, or the mini music player. `Ctrl+G` git, `Ctrl+\` terminal.

**Command palette** — `Ctrl+P`, fuzzy search over every module, theme and command. This is how the
system/admin modules are opened.

**Command mode** — `:` opens a vim-like command line.

**Popups** — image viewer, video player, PDF viewer and the calculator render on top of the previous
mode and restore it on `Esc`.

**Auto-detection** — in the file manager, `Enter` on a file picks the right module: audio →
music player, video → video player, image → image viewer, `.pdf` → PDF viewer, anything else →
editor. Entering a folder containing `.git` opens the Git side panel.

---

## Modules

Every module below is reachable from the command palette (`Ctrl+P`); the ones with a `:` command can
also be opened from command mode.

### Core

| Module | What it does | Open with |
|---|---|---|
| **Menu** | Home screen with animated logo | `F2`, `Esc` |
| **File Manager** | Browse, copy/cut/paste, rename, delete, bookmarks, hidden-file toggle, per-extension icons and colours, `/` search | menu |
| **Editor** | Text editor with undo/redo, markdown preview, `:w` / `:wq` / `:q` | `Enter` on a file |
| **Terminal** | Async integrated shell (`tokio::process`), persistent `cd`, command history, scroll mode (`Tab`) | `:terminal` |
| **Monitor** | Opens on a dashboard of cards — CPU (per-core grid + history), memory, swap, networking (per-second down/upload with per-interface rows) and storage split into main / USB / network volumes; `d` swaps to the process list with kill, sorting and filtering | `:ps` / `:sys` |
| **Git** | Full panel: status, stage/unstage, commit, pull, push, branches, log graph, `.gitignore` creation | `Ctrl+G`, `:git` |
| **Favorites** | Bookmarked files and folders (stored in the DB) | `:fav` |
| **Config** | In-app settings editor, writes `config/config.toml` | palette |
| **Theme Switcher** | Browse and apply themes live | `:theme` |
| **Help** | Built-in documentation: topics grouped by purpose in a left sidebar, per-topic tabs on the right, each split into bordered sub-panels (`↑/↓` topic · `←/→`/`Tab` tab · `PgUp/PgDn` scroll) | `F1`, `:help` |

### Media

| Module | What it does |
|---|---|
| **Audio Player** | Music library scanned from `music_dir` and indexed in the DB (lofty tags): Artists → Albums → Tracks drill-down, queue management, seek, volume, and a mini side panel that keeps playing while you work elsewhere |
| **Video Player** | Terminal video playback via `ffmpeg` with a background prefetch thread (8 frames ahead); Kitty native-resolution or ANSI half-block rendering |
| **Image Viewer** | PNG/JPEG/GIF/BMP/WebP/TIFF, Kitty or half-block |
| **PDF Viewer** | Scrollable text extraction via `pdftotext` |

### System

| Module | What it does |
|---|---|
| **Log Viewer** | Live tail of the app log with level filters (`e`/`w`/`i`/`0`) and follow mode (`f`) |
| **Service Manager** | systemd services — start (`s`), stop (`t`), restart (`r`), enable (`e`); falls back to a notice on non-systemd hosts |
| **Network** | Interfaces, IPv4 addresses and ping (parses `ip -o addr`, falls back to `ifconfig`) |
| **Disk Usage** | ncdu-style recursive analyser on a background scan thread, with delete |
| **Calculator** | Slide-up bottom sheet; own shunting-yard parser (`+ - * / % ^`, parentheses, unary minus) with history — `Ctrl+A` |

### Admin & remote

| Module | What it does |
|---|---|
| **Package Manager** | Wraps `brew` / `apt` / `dnf` / `pacman` — list installed, search, install/remove |
| **SSH Manager** | Hosts parsed from `~/.ssh/config`: real interactive sessions (the TUI suspends and restores), connectivity test, host groups, connection history, session tabs, local port-forward tunnels, and add/edit/remove entries (`~/.ssh/config` is backed up to `data/ssh_config.bak` before every write; passwords are never stored) |
| **SFTP** | Two-pane local/remote transfer over the `sftp` binary, all I/O on background threads |
| **Docker** | Containers, images and volumes via the `docker` CLI — start/stop/restart; graceful fallback when the daemon is down |
| **Cron** | Crontab editor: list, add, edit, delete jobs and write back with `w` |
| **Man Viewer** | Man pages with overstrike-aware formatting and `/` search (`n`/`N` to jump) |

### Apps

| Module | What it does |
|---|---|
| **Notes** | Markdown notes in `notes_dir`, indexed in the DB, live preview, embedded-image opening, fuzzy title search from the palette |
| **Weather** | Multi-city dashboard on open-meteo.com (no API key): current conditions with animated ASCII art, forecast, highlight grid (wind, UV, sunrise/sunset arc, humidity, visibility, feels-like), condition map, 15-minute cache, live geocoding search to add cities, °C/°F toggle |
| **Calendar** | Month grid with per-day tasks stored in the DB, inline task editor, done-toggle |

---

## Keybindings

### Global

| Key | Action |
|---|---|
| `F1` / `F2` / `F5` / `F10` | Help · Menu · Refresh · Quit |
| `Ctrl+P` | Command palette |
| `Ctrl+A` | Calculator (in the editor/terminal it is Select All instead) |
| `m` | Toggle the mini music panel (when audio is playing) |
| `Ctrl+T` / `Ctrl+-` / `Ctrl+W` | New tab · Close tab · Close tab (alias) |
| `Ctrl+1..9`, `Ctrl+0` | Switch tab (0 = tab 10) |
| `Alt+1..9` | Switch window |
| `Ctrl+G` / `Ctrl+\` | Git side panel · Side terminal |
| `Ctrl+S` / `Ctrl+Z` / `Ctrl+Y` | Save · Undo · Redo |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy · Cut · Paste |
| `Ctrl+F` | Find |
| `Ctrl+←` / `Ctrl+→` | Resize split |
| `Tab` / `Shift+Tab` | Next / previous focus |
| `:` | Command mode |
| `Esc` | Back / close |

No Vim modes — shortcuts are always active.

### File manager

`↑/↓` navigate · `Enter` open · `Backspace` parent · `b` bookmark · `B` favorites · `c/x/p` copy/cut/paste ·
`d`/`Del` delete · `r` rename · `h` hidden files · `/` search

### Audio player

`Space` play/pause · `Tab` library ↔ queue · `Enter` drill down / play · `Backspace` back ·
`a`/`d` add to / remove from queue · `n`/`p` next/prev · `+`/`-` volume · `←/→` seek 10s ·
`F5` rescan library · `m` minimize to mini panel · `Esc` background (keeps playing)

### Video player

`Space` play/pause · `←/→` seek 5s · `Esc`/`q` close

### Git

`a` stage · `u` unstage · `c` commit · `b` branches · `l` log graph · `i` create `.gitignore` ·
`p` pull · `P` push · `Tab` switch pane · `F5` refresh

### Monitor

Opens on the dashboard. `d` swaps to the process list · `↑/↓` navigate · `k` kill · `1-4` sort column ·
`/` filter · `F5` refresh · `Esc` back to the dashboard, then to the menu

Per-mode key hints are always shown in the hint bar, and `F1` opens the full reference.

## `:` commands

| Command | Action |
|---|---|
| `:q` · `:quit` · `:wq` | Quit |
| `:w` · `:save` | Save |
| `:open <path>` | Open a file |
| `:split h` · `:split v` | Split the panel |
| `:theme` · `:theme <name>` | Theme switcher · set theme |
| `:git [status\|add <f>\|commit <msg>\|pull\|push]` | Git operations |
| `:terminal` · `:term` | Integrated terminal |
| `:ps` · `:proc` · `:sys` · `:sysmon` · `:monitor` | Monitor |
| `:fav` · `:favorites` | Favorites |
| `:notes` · `:weather` · `:wttr` · `:calendar` · `:cal` · `:ssh` | Open that app |
| `:help` | Help |
| `:plugin list` | List loaded plugins |

---

## Persistence

`data/app.db` (SQLite, bundled rusqlite) stores: `sessions`, `recent_files`, `command_history`,
`app_logs`, `favorites`, `music_library`, `notes_index`, `tasks`, `weather_cities`, `ssh_history`,
`ssh_groups`, `ssh_host_groups`.

## Graphics

`kitty.rs` implements the Kitty graphics protocol. Detection happens once at startup; when the
terminal supports it, images and video are drawn at real pixel resolution (the frame is injected
right after ratatui paints, so the reserved area exists first). Everywhere else VOS falls back to
ANSI half-block rendering, so nothing is ever unavailable — only lower resolution.

## Plugins

`plugins/` defines a minimal `Plugin` trait (`name()` + `render(f, area)`) held by a
`PluginRegistry`. The Git plugin is registered automatically when the working directory is a repo.

## Layout

```
src/
├── main.rs        # event loop (tokio::select! over crossterm events + 16ms tick)
├── app.rs         # App — central state, tick(), execute_command()
├── core/          # state/modes, keybinds, command parser, event bus, errors
├── events/        # input dispatch (modal → side panel → global binds → per-mode)
├── ui/            # one render_* panel per module + theme, icons, layout, status bar
├── modules/       # module logic (no ratatui): sysmon, docker, ssh, weather, cron, …
├── wm/            # windows, tabs, splits
├── editor/ fs/ terminal/ audio/ video/ db/ session/ config/ plugins/
└── kitty.rs       # Kitty graphics protocol
```

`Doc/PLAN.md` and `Doc/ROADMAP.md` hold the long-form design; `CLAUDE.md` documents the internals for
contributors.
