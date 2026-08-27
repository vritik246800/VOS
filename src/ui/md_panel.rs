use crate::editor::buffer::Buffer;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render_md_panel(f: &mut Frame, area: Rect, buffer: &Buffer, focused: bool) {
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let name = buffer
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled");
    let title = format!(" {name} [VIEW]  Ctrl+E=edit ");

    let content = buffer.lines.join("\n");
    let rendered = parse_markdown(&content);

    let para = Paragraph::new(rendered)
        .scroll((buffer.md_scroll as u16, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(border_color)),
        );

    f.render_widget(para, area);
}

pub fn parse_markdown(content: &str) -> Vec<Line<'static>> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(content, opts);

    let mut result: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default()];

    // (ordered, next_number)
    let mut list_stack: Vec<(bool, u64)> = Vec::new();

    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines: Vec<String> = Vec::new();

    let mut bq_depth: usize = 0;

    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut in_table = false;
    let mut in_table_head = false;

    for event in parser {
        match event {
            // ── Block opens ──────────────────────────────────────────────────
            Event::Start(Tag::Heading(level, ..)) => {
                let (color, prefix): (Color, &'static str) = match level {
                    HeadingLevel::H1 => (Color::Yellow, "# "),
                    HeadingLevel::H2 => (Color::Green, "## "),
                    HeadingLevel::H3 => (Color::Cyan, "### "),
                    HeadingLevel::H4 => (Color::LightBlue, "#### "),
                    HeadingLevel::H5 => (Color::Magenta, "##### "),
                    HeadingLevel::H6 => (Color::Gray, "###### "),
                };
                let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                style_stack.push(style);
                current_spans.push(Span::styled(prefix, style));
            }
            Event::End(Tag::Heading(..)) => {
                style_stack.pop();
                flush(&mut current_spans, &mut result);
                result.push(Line::from(""));
            }

            Event::Start(Tag::Paragraph) => {
                if bq_depth > 0 {
                    let pfx = "▌ ".repeat(bq_depth);
                    current_spans.push(Span::styled(pfx, Style::default().fg(Color::Blue)));
                }
            }
            Event::End(Tag::Paragraph) => {
                flush(&mut current_spans, &mut result);
                result.push(Line::from(""));
            }

            Event::Start(Tag::BlockQuote) => {
                bq_depth += 1;
            }
            Event::End(Tag::BlockQuote) => {
                bq_depth = bq_depth.saturating_sub(1);
                if bq_depth == 0 {
                    result.push(Line::from(""));
                }
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code_lines.clear();
            }
            Event::End(Tag::CodeBlock(_)) => {
                in_code_block = false;
                emit_code_block(&code_lang, &code_lines, &mut result);
                code_lines.clear();
                code_lang.clear();
            }

            Event::Start(Tag::List(start)) => {
                list_stack.push((start.is_some(), start.unwrap_or(1)));
            }
            Event::End(Tag::List(_)) => {
                list_stack.pop();
                if list_stack.is_empty() {
                    result.push(Line::from(""));
                }
            }

            Event::Start(Tag::Item) => {
                let depth = list_stack.len();
                let indent = "  ".repeat(depth.saturating_sub(1));
                if bq_depth > 0 {
                    let pfx = "▌ ".repeat(bq_depth);
                    current_spans.push(Span::styled(pfx, Style::default().fg(Color::Blue)));
                }
                let bullet = if let Some((ordered, count)) = list_stack.last_mut() {
                    if *ordered {
                        let n = *count;
                        *count += 1;
                        format!("{indent}{n}. ")
                    } else {
                        format!("{indent}● ")
                    }
                } else {
                    format!("{indent}● ")
                };
                current_spans.push(Span::styled(bullet, Style::default().fg(Color::Cyan)));
            }
            Event::End(Tag::Item) => {
                if !current_spans.is_empty() {
                    flush(&mut current_spans, &mut result);
                }
            }

            // ── Inline opens/closes ─────────────────────────────────────────
            Event::Start(Tag::Strong) => {
                let s = top(&style_stack).add_modifier(Modifier::BOLD);
                style_stack.push(s);
            }
            Event::End(Tag::Strong) => {
                style_stack.pop();
            }

            Event::Start(Tag::Emphasis) => {
                let s = top(&style_stack)
                    .add_modifier(Modifier::ITALIC)
                    .fg(Color::LightYellow);
                style_stack.push(s);
            }
            Event::End(Tag::Emphasis) => {
                style_stack.pop();
            }

            Event::Start(Tag::Strikethrough) => {
                let s = top(&style_stack)
                    .add_modifier(Modifier::CROSSED_OUT)
                    .fg(Color::DarkGray);
                style_stack.push(s);
            }
            Event::End(Tag::Strikethrough) => {
                style_stack.pop();
            }

            Event::Start(Tag::Link(_, _, _)) => {
                let s = top(&style_stack)
                    .fg(Color::Blue)
                    .add_modifier(Modifier::UNDERLINED);
                style_stack.push(s);
            }
            Event::End(Tag::Link(_, _, _)) => {
                style_stack.pop();
            }

            Event::Start(Tag::Image(_, _, _)) => {
                current_spans.push(Span::styled(
                    "[img] ".to_string(),
                    Style::default().fg(Color::Magenta),
                ));
                let s = top(&style_stack).fg(Color::Magenta);
                style_stack.push(s);
            }
            Event::End(Tag::Image(_, _, _)) => {
                style_stack.pop();
            }

            // ── Table ───────────────────────────────────────────────────────
            Event::Start(Tag::Table(_)) => {
                in_table = true;
                table_rows.clear();
            }
            Event::End(Tag::Table(_)) => {
                in_table = false;
                emit_table(&table_rows, &mut result);
                table_rows.clear();
                result.push(Line::from(""));
            }
            Event::Start(Tag::TableHead) => {
                in_table_head = true;
                current_row.clear();
            }
            Event::End(Tag::TableHead) => {
                in_table_head = false;
                table_rows.push(std::mem::take(&mut current_row));
                table_rows.push(vec!["__SEP__".to_string()]);
            }
            Event::Start(Tag::TableRow) => {
                current_row.clear();
            }
            Event::End(Tag::TableRow) => {
                if !in_table_head {
                    table_rows.push(std::mem::take(&mut current_row));
                }
            }
            Event::Start(Tag::TableCell) => {
                current_cell.clear();
            }
            Event::End(Tag::TableCell) => {
                current_row.push(std::mem::take(&mut current_cell));
            }

            // ── Text content ────────────────────────────────────────────────
            Event::Text(text) => {
                let s = text.to_string();
                if in_code_block {
                    for line in s.lines() {
                        code_lines.push(line.to_string());
                    }
                } else if in_table {
                    current_cell.push_str(&s);
                } else {
                    let style = top(&style_stack);
                    for span in wikilinks(&s, style) {
                        current_spans.push(span);
                    }
                }
            }

            Event::Code(code) => {
                current_spans.push(Span::styled(
                    format!(" {} ", code),
                    Style::default().fg(Color::LightGreen).bg(Color::Black),
                ));
            }

            Event::SoftBreak => {
                current_spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                flush(&mut current_spans, &mut result);
            }

            Event::Rule => {
                result.push(Line::from(Span::styled(
                    "─".repeat(60),
                    Style::default().fg(Color::DarkGray),
                )));
                result.push(Line::from(""));
            }

            Event::TaskListMarker(checked) => {
                let (marker, color) = if checked {
                    ("☑ ", Color::Green)
                } else {
                    ("☐ ", Color::Gray)
                };
                current_spans.push(Span::styled(marker.to_string(), Style::default().fg(color)));
            }

            _ => {}
        }
    }

    if !current_spans.is_empty() {
        result.push(Line::from(std::mem::take(&mut current_spans)));
    }

    result
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn top(stack: &[Style]) -> Style {
    stack.last().copied().unwrap_or_default()
}

fn flush(spans: &mut Vec<Span<'static>>, result: &mut Vec<Line<'static>>) {
    if !spans.is_empty() {
        result.push(Line::from(std::mem::take(spans)));
    }
}

fn wikilinks(text: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = text.to_string();

    loop {
        if let Some(start) = rest.find("[[") {
            if let Some(inner_end) = rest[start + 2..].find("]]") {
                let before = rest[..start].to_string();
                if !before.is_empty() {
                    spans.push(Span::styled(before, base));
                }
                let inner = rest[start + 2..start + 2 + inner_end].to_string();
                spans.push(Span::styled(
                    format!("[[{inner}]]"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED),
                ));
                rest = rest[start + 2 + inner_end + 2..].to_string();
                continue;
            }
        }
        break;
    }

    if !rest.is_empty() {
        spans.push(Span::styled(rest, base));
    }
    if spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
    }
    spans
}

fn emit_code_block(lang: &str, lines: &[String], result: &mut Vec<Line<'static>>) {
    let is_mermaid = lang.trim().eq_ignore_ascii_case("mermaid");
    let (border_col, label_col) = if is_mermaid {
        (Color::Magenta, Color::LightMagenta)
    } else {
        (Color::DarkGray, Color::Yellow)
    };
    let label = if lang.is_empty() { "code" } else { lang };
    let bar = "─".repeat(32usize.saturating_sub(label.len() + 4).max(2));

    result.push(Line::from(vec![
        Span::styled(
            format!("┌─ {label} "),
            Style::default().fg(label_col).add_modifier(Modifier::BOLD),
        ),
        Span::styled(bar.clone(), Style::default().fg(border_col)),
    ]));
    for l in lines {
        result.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(border_col)),
            Span::styled(l.clone(), Style::default().fg(Color::LightGreen)),
        ]));
    }
    result.push(Line::from(Span::styled(
        format!("└{}", "─".repeat(bar.len() + label.len() + 4)),
        Style::default().fg(border_col),
    )));
    result.push(Line::from(""));
}

fn emit_table(rows: &[Vec<String>], result: &mut Vec<Line<'static>>) {
    let data: Vec<&Vec<String>> = rows
        .iter()
        .filter(|r| !(r.len() == 1 && r[0] == "__SEP__"))
        .collect();
    if data.is_empty() {
        return;
    }

    let ncols = data.iter().map(|r| r.len()).max().unwrap_or(0);
    if ncols == 0 {
        return;
    }

    let mut col_w: Vec<usize> = vec![3; ncols];
    for row in &data {
        for (i, cell) in row.iter().enumerate() {
            if i < ncols {
                col_w[i] = col_w[i].max(cell.len() + 2);
            }
        }
    }

    let border = Style::default().fg(Color::DarkGray);

    let top = format!(
        "┌{}┐",
        col_w
            .iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join("┬")
    );
    result.push(Line::from(Span::styled(top, border)));

    let mut is_header = true;
    for row in rows {
        if row.len() == 1 && row[0] == "__SEP__" {
            let sep = format!(
                "├{}┤",
                col_w
                    .iter()
                    .map(|w| "─".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("┼")
            );
            result.push(Line::from(Span::styled(sep, border)));
            is_header = false;
            continue;
        }
        let mut spans: Vec<Span<'static>> = vec![Span::styled("│".to_string(), border)];
        for (i, cell) in row.iter().enumerate() {
            if i >= ncols {
                break;
            }
            let padded = format!(" {:<width$}", cell, width = col_w[i].saturating_sub(1));
            let cell_style = if is_header {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            spans.push(Span::styled(padded, cell_style));
            spans.push(Span::styled("│".to_string(), border));
        }
        result.push(Line::from(spans));
    }

    let bottom = format!(
        "└{}┘",
        col_w
            .iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join("┴")
    );
    result.push(Line::from(Span::styled(bottom, border)));
}
