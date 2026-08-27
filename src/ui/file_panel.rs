use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::path::Path;

use nerd_font_symbols::seti::{
    CUSTOM_C, CUSTOM_CPP, CUSTOM_FOLDER, CUSTOM_FOLDER_CONFIG, CUSTOM_FOLDER_GIT,
    CUSTOM_FOLDER_GITHUB, CUSTOM_FOLDER_NPM, CUSTOM_KOTLIN, CUSTOM_RUBY, CUSTOM_TOML, SETI_AUDIO,
    SETI_C, SETI_C_SHARP, SETI_CONFIG, SETI_CPP, SETI_CSS, SETI_DART, SETI_DB, SETI_DEFAULT,
    SETI_DOCKER, SETI_GIT, SETI_GO, SETI_GRAPHQL, SETI_HASKELL, SETI_HTML, SETI_IMAGE, SETI_JAVA,
    SETI_JAVASCRIPT, SETI_JSON, SETI_JULIA, SETI_LOCK, SETI_LUA, SETI_MARKDOWN, SETI_PDF, SETI_PHP,
    SETI_POWERSHELL, SETI_PYTHON, SETI_REACT, SETI_RUST, SETI_SASS, SETI_SCALA, SETI_SHELL,
    SETI_SVELTE, SETI_SVG, SETI_SWIFT, SETI_TYPESCRIPT, SETI_VIDEO, SETI_VUE, SETI_XML, SETI_YML,
    SETI_ZIP,
};
use nerdicons_rs::cod::{RSFILE, RSFOLDER, RSFOLDER_OPENED, RSMUSIC};

use crate::fs::explorer::FileExplorer;

/// Returns the list area so callers can cache it for mouse hit-testing.
pub fn render_file_panel(
    f: &mut Frame,
    area: Rect,
    explorer: &FileExplorer,
    focused: bool,
    cut_path: Option<&Path>,
) -> Rect {
    let selected_is_git_dir = explorer
        .selected_entry()
        .map(|e| e.is_dir && e.path.join(".git").exists())
        .unwrap_or(false);

    let (list_area, hint_area) = if selected_is_git_dir {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let border_color = if focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let path_str = explorer.current_dir.display().to_string();
    let title = format!(" Files: {} ", path_str);

    let items: Vec<ListItem> = explorer
        .entries
        .iter()
        .map(|e| {
            let is_cut = cut_path.map_or(false, |p| p == e.path);
            let (icon, icon_color) = file_icon(&e.name, e.is_dir);
            let size_str = if e.is_dir {
                "       ".to_string()
            } else {
                format!("{:>7}", format_size(e.size))
            };
            let name_fg = if is_cut {
                Color::Yellow
            } else if e.is_dir {
                Color::Cyan
            } else {
                Color::White
            };
            let icon_fg = if is_cut { Color::Yellow } else { icon_color };
            let mut name_style = Style::default().fg(name_fg);
            if is_cut {
                name_style = name_style.add_modifier(Modifier::DIM);
            }
            let icon_str = format!("{} ", icon);
            let name_str = format!("{:<38}", e.name);
            let line = Line::from(vec![
                Span::styled(icon_str, Style::default().fg(icon_fg)),
                Span::styled(name_str, name_style),
                Span::styled(size_str, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = explorer.list_state.clone();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title.as_str())
                .border_style(Style::default().fg(border_color)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, list_area, &mut state);

    if let Some(hint) = hint_area {
        let hint_widget = Paragraph::new(" [ G ] Open Git panel ")
            .style(Style::default().fg(Color::Black).bg(Color::Yellow));
        f.render_widget(hint_widget, hint);
    }

    list_area
}

fn file_icon(name: &str, is_dir: bool) -> (&'static str, Color) {
    if is_dir {
        return dir_icon(name);
    }
    // Special full filenames first
    match name {
        "Dockerfile" | "dockerfile" => return (SETI_DOCKER, Color::Rgb(1, 133, 198)),
        ".gitignore" | ".gitattributes" | ".gitmodules" => {
            return (SETI_GIT, Color::Rgb(245, 77, 39));
        }
        "Makefile" | "makefile" | "GNUmakefile" => return (SETI_CONFIG, Color::Rgb(142, 142, 142)),
        "Cargo.toml" | "Cargo.lock" => return (SETI_RUST, Color::Rgb(222, 165, 132)),
        "package.json" | "package-lock.json" => return (SETI_JAVASCRIPT, Color::Rgb(203, 203, 65)),
        "tsconfig.json" => return (SETI_TYPESCRIPT, Color::Rgb(49, 120, 198)),
        "docker-compose.yml" | "docker-compose.yaml" => {
            return (SETI_DOCKER, Color::Rgb(1, 133, 198));
        }
        ".env" | ".env.local" | ".env.production" => {
            return (SETI_CONFIG, Color::Rgb(142, 142, 142));
        }
        "LICENSE" | "LICENSE.md" | "LICENCE" => return (SETI_LOCK, Color::Rgb(152, 195, 121)),
        _ => {}
    }

    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        // Rust
        "rs" => (SETI_RUST, Color::Rgb(222, 165, 132)),
        // Python
        "py" | "pyw" | "pyi" => (SETI_PYTHON, Color::Rgb(53, 114, 165)),
        // JavaScript
        "js" | "mjs" | "cjs" => (SETI_JAVASCRIPT, Color::Rgb(203, 203, 65)),
        // TypeScript
        "ts" | "mts" | "cts" => (SETI_TYPESCRIPT, Color::Rgb(49, 120, 198)),
        // JSX / TSX (React)
        "jsx" | "tsx" => (SETI_REACT, Color::Rgb(97, 218, 251)),
        // Vue / Svelte
        "vue" => (SETI_VUE, Color::Rgb(65, 184, 131)),
        "svelte" => (SETI_SVELTE, Color::Rgb(255, 62, 0)),
        // Go
        "go" => (SETI_GO, Color::Rgb(0, 173, 216)),
        // Java / Kotlin
        "java" => (SETI_JAVA, Color::Rgb(176, 114, 25)),
        "kt" | "kts" => (CUSTOM_KOTLIN, Color::Rgb(169, 105, 234)),
        // C / C++
        "c" | "h" => (SETI_C, Color::Rgb(0, 119, 185)),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => (SETI_CPP, Color::Rgb(0, 119, 185)),
        // C#
        "cs" => (SETI_C_SHARP, Color::Rgb(105, 56, 198)),
        // PHP
        "php" => (SETI_PHP, Color::Rgb(119, 123, 179)),
        // Ruby
        "rb" | "rake" | "gemspec" => (CUSTOM_RUBY, Color::Rgb(193, 55, 55)),
        // Swift
        "swift" => (SETI_SWIFT, Color::Rgb(247, 94, 24)),
        // Lua
        "lua" => (SETI_LUA, Color::Rgb(0, 0, 128)),
        // Haskell
        "hs" | "lhs" => (SETI_HASKELL, Color::Rgb(94, 80, 134)),
        // Scala
        "scala" | "sc" => (SETI_SCALA, Color::Rgb(220, 50, 47)),
        // Dart
        "dart" => (SETI_DART, Color::Rgb(0, 180, 216)),
        // Julia
        "jl" => (SETI_JULIA, Color::Rgb(149, 88, 178)),
        // GraphQL
        "graphql" | "gql" => (SETI_GRAPHQL, Color::Rgb(229, 53, 171)),
        // HTML
        "html" | "htm" | "xhtml" => (SETI_HTML, Color::Rgb(228, 77, 38)),
        // CSS / SCSS / SASS
        "css" => (SETI_CSS, Color::Rgb(38, 114, 213)),
        "scss" | "sass" => (SETI_SASS, Color::Rgb(205, 103, 153)),
        // Config / Data formats
        "json" => (SETI_JSON, Color::Rgb(203, 203, 65)),
        "yaml" | "yml" => (SETI_YML, Color::Rgb(200, 85, 85)),
        "xml" => (SETI_XML, Color::Rgb(228, 77, 38)),
        "toml" => (CUSTOM_TOML, Color::Rgb(159, 110, 245)),
        "ini" | "cfg" | "conf" => (SETI_CONFIG, Color::Rgb(142, 142, 142)),
        // Shell
        "sh" | "bash" | "zsh" | "fish" => (SETI_SHELL, Color::Rgb(142, 198, 108)),
        "ps1" | "psm1" | "psd1" => (SETI_POWERSHELL, Color::Rgb(1, 36, 86)),
        // Docs
        "md" | "markdown" | "mdx" => (SETI_MARKDOWN, Color::Rgb(71, 101, 150)),
        // Images
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" | "tif" | "ico" | "psd" | "ai"
        | "xcf" => (SETI_IMAGE, Color::Rgb(162, 205, 90)),
        "svg" => (SETI_SVG, Color::Rgb(255, 186, 0)),
        // Video
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "mpeg" | "mpg" | "ogv" => {
            (SETI_VIDEO, Color::Rgb(224, 108, 117))
        }
        // Audio
        "mp3" | "flac" | "wav" | "ogg" | "aac" | "m4a" | "opus" | "wma" => {
            (SETI_AUDIO, Color::Rgb(136, 77, 201))
        }
        // PDF
        "pdf" => (SETI_PDF, Color::Rgb(220, 50, 47)),
        // Archives
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "tgz" | "tbz2" => {
            (SETI_ZIP, Color::Rgb(221, 182, 117))
        }
        // Database
        "db" | "sqlite" | "sqlite3" | "sql" => (SETI_DB, Color::Rgb(0, 104, 151)),
        // Docker
        "dockerfile" => (SETI_DOCKER, Color::Rgb(1, 133, 198)),
        // Lock files
        "lock" => (SETI_LOCK, Color::Rgb(142, 142, 142)),
        _ => (SETI_DEFAULT, Color::Gray),
    }
}

fn dir_icon(name: &str) -> (&'static str, Color) {
    match name {
        ".git" => (CUSTOM_FOLDER_GIT, Color::Rgb(245, 77, 39)),
        ".github" | ".gitlab" => (CUSTOM_FOLDER_GITHUB, Color::Rgb(121, 131, 148)),
        "node_modules" => (CUSTOM_FOLDER_NPM, Color::Rgb(106, 153, 85)),
        ".config" | "config" | "conf" => (CUSTOM_FOLDER_CONFIG, Color::Rgb(142, 142, 142)),
        "src" | "lib" | "include" => (RSFOLDER, Color::Rgb(124, 175, 194)),
        "docs" | "doc" | "documentation" => (RSFOLDER, Color::Rgb(110, 180, 208)),
        "tests" | "test" | "spec" => (RSFOLDER, Color::Rgb(152, 195, 121)),
        "dist" | "build" | "out" => (RSFOLDER, Color::Rgb(229, 192, 123)),
        "target" => (RSFOLDER, Color::Rgb(152, 100, 100)),
        _ => (CUSTOM_FOLDER, Color::Rgb(232, 197, 51)),
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}
