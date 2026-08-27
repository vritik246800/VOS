# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Rule: keep README.md in sync

`README.md` (in English, at the VOS root) is the user-facing description of what VOS can do.
**Whenever you add or change a user-visible capability, update `README.md` in the same task, before
calling the task done.**

Counts as a capability change:

- A new module / `AppMode`, or a module removed or renamed
- New or changed keybindings, `:` commands or command-palette entries
- New or changed `config.toml` settings, DB tables or runtime dependencies (external binaries)
- A module gaining or losing a significant feature (a new panel, a new backend, a new integration)

Does not count (leave the README alone): internal refactors, bug fixes, tests, rendering tweaks that
do not change behaviour, generated files.

Where to write it: add to the matching module table (Core / Media / System / Admin & remote / Apps),
the keybinding section and the `:` command table. Describe what the code does today — do not
document planned features from `Doc/PLAN.md` / `Doc/ROADMAP.md`. Write in English, matching the
existing section structure; only create a new section if nothing existing fits.

The parent repo's `README.md` (`Rust_Projects/README.md`) also has a VOS section — update it only if
the change alters the summary there (stack, module list, notable features).

## Build / run / test

```bash
cargo run            # run from repo root (reads config/config.toml, opens data/app.db)
cargo build
cargo build --release
cargo check          # fast type-check without linking
cargo test
cargo fmt
cargo clippy
```

Always run from the repository root — the app resolves `config/config.toml` and `data/app.db` relative to CWD.

## Runtime dependencies

- `ffmpeg` + `ffprobe` in `$PATH` — video player (frame extraction + probing)
- Audio device — audio player (rodio)
- Kitty terminal — optional; enables native-resolution image/video rendering

## Architecture

### Event loop (`main.rs`)

```
tokio::select! {
    crossterm event → handle_input(app, event)   [events/input.rs]
    16ms tick       → app.tick()                  [app.rs]
}
terminal.draw(|f| render(f, app))
// AFTER draw: inject pending Kitty frame
if let Some(frame) = app.pending_kitty.take() { kitty::inject_frame(&frame) }
```

The Kitty frame is injected **after** `terminal.draw()` completes — ratatui must paint first so the reserved black area exists before Kitty writes pixels over it.

### Central state (`App` in `app.rs`)

`App` owns all state. `render_*()` functions take `&App` (or `&mut App` for widgets needing `ListState`) — they never mutate application state. All state mutations go through `handle_input()` → `app.execute_command()` or direct field writes in `handle_input`.

### Mode system (`core/state.rs`)

`AppState::set_mode()` saves `prev_mode` before switching. Three modes are **popup overlays** — they render over `prev_mode`'s content:

- `AppMode::ImageViewer` — static image via Kitty or half-block
- `AppMode::VideoPlayer` — video via Kitty or half-block; `prev_mode` restored on close
- `AppMode::PdfViewer` — scrollable text pages

Full mode list (`core/state.rs`): `Menu | FileManager | Editor | Terminal | ProcessViewer | Git | Config | AudioPlayer | VideoPlayer | ImageViewer | PdfViewer | Favorites | Help | ThemeSwitcher | LogViewer | ServiceManager | NetworkPanel | DiskManager | Calculator | PackageManager | SshManager | SshConnectForm | SftpPanel | DockerPanel | CronEditor | ManViewer | Notes | Weather | Calendar | CommandPalette | Command(String) | Dialog(DialogKind) | Quitting`

### Input dispatch (`events/input.rs`)

`handle_input` dispatches in priority order:
1. Modal overrides: `Command(input)`, `CommandPalette`, `Dialog(_)`
2. Side panel focus (`app.state.side_focused`)
3. Global keybind engine (`core/keybinds.rs`) → `Action` enum
4. Per-mode handlers (`handle_file_manager`, `handle_editor`, etc.)

### Kitty graphics pipeline (`kitty.rs`)

`app.use_kitty` is set once at startup via env-var detection. `app.kitty_cell_px: (u16, u16)` holds the pixel size of one terminal cell, queried via `crossterm::terminal::window_size()` (falls back to `(14, 28)` for Retina).

For **video**, `VideoPlayer.cell_px` controls the ffmpeg decode resolution:
- `(1, 2)` → half-block mode: ffmpeg decodes at `(width_chars × height_chars*2)` px
- real cell size → Kitty native: ffmpeg decodes at `(width_chars × cell_px_w) × (height_chars × cell_px_h)` px — no upscaling in the terminal

`app.pending_kitty: Option<KittyFrame>` is set during the draw phase; `main.rs` drains it immediately after `terminal.draw()`. `image_id=1` = image viewer, `image_id=2` = video player (stable IDs allow in-place frame replacement without flicker).

### Side panel

Animated slide-in: `state.side_pct_target` is set to 40 (open) or 0 (close); `tick_side_panel()` increments/decrements `side_pct` by 4 each tick toward target. `SidePanelMode`: `None | Git | Terminal | SystemMonitor`.

### Command palette / command mode

`Ctrl+P` opens palette (fuzzy search over actions). `:` enters `AppMode::Command(String)` — text is parsed by `core/command::CommandParser` into the `Command` enum (`q`/`quit`, `w`/`save`, `open <path>`, `split h|v`, `theme <name>`, `git status|add|commit|pull|push`, etc.).

### Video prefetch

`VideoPlayer` maintains a background thread that pre-decodes `PREFETCH_AHEAD=8` frames into a `FrameBuffer` (Arc<Mutex<VecDeque<RawFrame>>>). `advance_frame()` pulls from this buffer; falls back to synchronous ffmpeg on cache miss. ANSI half-block render cache (`render_cache`) is skipped in Kitty mode — only raw pixel data is stored.

### Plugin system (`plugins/`)

`Plugin` trait: `name() -> &str` + `render(f, area)`. `PluginRegistry` holds `Vec<Box<dyn Plugin>>`. Currently only `GitPlugin` is registered (auto-detected from CWD). To add a plugin: implement `Plugin`, push to `app.plugins` in `App::new()`.

## Keybindings

No Vim modes. Direct shortcuts always active:

| Key | Action |
|-----|--------|
| `Ctrl+P` | Command Palette |
| `Ctrl+S` | Save |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `Ctrl+C/X/V` | Copy / Cut / Paste |
| `Ctrl+F` | Search |
| `Ctrl+T` / `Ctrl+W` | New tab / Close tab |
| `Ctrl+1–9` | Switch tab |
| `Alt+1–9` | Switch window |
| `Tab` / `Shift+Tab` | Navigate panels |
| `F1` Help · `F2` Menu · `F5` Refresh · `F10` Quit | |
| `Esc` | Cancel / back |

### Icon system (`ui/icons.rs`)

All Nerd Font icons are centralised in `src/ui/icons.rs` — **never use raw `\u{xxxx}` escapes in panel files**.

**Crates:**
- `nerd-font-symbols` — provides named constants organised by icon set (`md::`, `seti::`, `fa::`, etc.)
- `nerdicons_rs` — provides codicon constants (`cod::RSFOLDER`, `cod::RSFILE`, etc.)

**Usage pattern** — import semantic aliases from `crate::ui::icons`:

```rust
use crate::ui::icons::{ICON_PLAY, ICON_PAUSE, ICON_CPU, ICON_MODE_FILES};
```

**Semantic aliases defined in `icons.rs`:**

| Alias | Source constant | Used in |
|-------|----------------|---------|
| `ICON_INFO / WARNING / ERROR / SUCCESS` | `MD_INFORMATION / ALERT / ALERT_CIRCLE / CHECK` | notifications, log |
| `ICON_PLAY / PAUSE / NEXT / PREV` | `MD_PLAY / PAUSE / SKIP_NEXT / SKIP_PREVIOUS` | audio, video |
| `ICON_MUSIC_NOTE` | `MD_MUSIC` | audio, status bar |
| `ICON_CPU / RAM / DISK / NETWORK` | `MD_CHIP / MEMORY / HARDDISK / NETWORK` | sysmon |
| `ICON_NETWORK_RX / TX` | `MD_ARROW_DOWN / UP` | network, sysmon |
| `ICON_ACTIVE / INACTIVE / UNKNOWN` | `MD_CHECK_CIRCLE / CLOSE / HELP_CIRCLE` | network, ssh |
| `ICON_STAR / DIR / FILE_GENERIC` | `MD_STAR / RSFOLDER / RSFILE` | favorites |
| `ICON_MODE_*` (25 modes) | `MD_HOME / FOLDER / PENCIL / COG / POWER …` | status bar |
| `ICON_HIGHLIGHT` | `MD_PLAY` | list highlight symbol |

**File-type icons** (`file_panel.rs`) use `SETI_*` and `CUSTOM_*` constants from `nerd_font_symbols::seti` mapped by extension, with per-type `Color::Rgb` colours. To add a new extension:

```rust
// in file_panel.rs → file_icon() match arm:
"ext" => (SETI_SOME_ICON, Color::Rgb(r, g, b)),
```

**Requires a Nerd Font** in the terminal (e.g. JetBrains Mono Nerd Font, Hack Nerd Font) for glyphs to render correctly.

## Implementation notes

- **No UI mutations**: `render_*()` functions are read-only; all state changes happen in `handle_input` or `app.tick()`.
- **Async model**: `tokio::select!` over crossterm `EventStream` + 16ms interval timer. Internal terminal uses `tokio::process::Command` with piped stdout via mpsc channel.
- **Persistence**: `data/app.db` (SQLite via rusqlite) stores sessions, recent files, command history, logs. Sessions table: `sessions`. Config: `config/config.toml` (auto-created on first run).
- **Image loading**: done in `handle_input` (blocking) — image is decoded once, stored as raw RGB24 in `app.image_rgb`, resized to fit the popup area capped at `(800, 600)`.
