use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    // Rect kept: used for the version badge overlay
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap},
    Terminal,
};

// ── color palette ─────────────────────────────────────────────────────────────────
const ACCENT: Color = Color::Cyan;
const ACCENT2: Color = Color::Magenta;
const SUCCESS: Color = Color::Green;
const MUTED: Color = Color::Rgb(100, 116, 139);
const SURFACE: Color = Color::Rgb(15, 23, 42);
const ON_SURFACE: Color = Color::Rgb(226, 232, 240);

// ── helpers ──────────────────────────────────────────────────────────────────

fn inner_block<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left)
        .padding(Padding::horizontal(1))
}

// ── help screen ──────────────────────────────────────────────────────────────

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn show_help_tui() -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Spinner state for the "animated" title
    let spinner_frames = ['◐', '◓', '◑', '◒'];
    let mut frame_idx: usize = 0;

    loop {
        let spin_char = spinner_frames[frame_idx % spinner_frames.len()];

        terminal.draw(|frame| {
            let area = frame.area();

            // ── root layout ──────────────────────────────────────────────
            // outer padding
            let outer = area.inner(Margin { vertical: 1, horizontal: 2 });

            // top-bar / content / status-bar
            let root = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),  // header bar
                    Constraint::Min(1),     // body
                    Constraint::Length(3),  // status bar
                ])
                .split(outer);

            // ── header bar ───────────────────────────────────────────────
            let header = Paragraph::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("s", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("i", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("f", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("t", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("  | ", Style::default()),
                Span::styled(
                    "Strip noise from error output, all done locally",
                    Style::default().fg(ON_SURFACE),
                ),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(ACCENT))
                    .style(Style::default().bg(SURFACE)),
            )
            .alignment(Alignment::Left);
            frame.render_widget(header, root[0]);

            // ── body: two columns ─────────────────────────────────────────
            let body_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(48),
                    Constraint::Length(2),   // gutter
                    Constraint::Percentage(50),
                ])
                .split(root[1]);

            // left column: USAGE
            let usage_lines = vec![
                Line::raw(""),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("command", Style::default().fg(Color::Yellow)),
                    Span::styled(" 2>&1 | ", Style::default().fg(MUTED)),
                    Span::styled("sift", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled(
                        "    → clean, readable error summary",
                        Style::default().fg(MUTED),
                    ),
                ]),
                Line::raw(""),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("command", Style::default().fg(Color::Yellow)),
                    Span::styled(" 2>&1 | ", Style::default().fg(MUTED)),
                    Span::styled("sift -s", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled(
                        "    → search query for the error",
                        Style::default().fg(MUTED),
                    ),
                ]),
                Line::raw(""),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("sift", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                    Span::styled(" < error.log", Style::default().fg(Color::Yellow)),
                ]),
                Line::from(vec![
                    Span::styled(
                        "    → read from a file",
                        Style::default().fg(MUTED),
                    ),
                ]),
                Line::raw(""),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("sift -v", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                    Span::styled(" < error.log", Style::default().fg(Color::Yellow)),
                ]),
                Line::from(vec![
                    Span::styled(
                        "    → verbose: both clean + search",
                        Style::default().fg(MUTED),
                    ),
                ]),
                Line::raw(""),
            ];
            let usage_panel = Paragraph::new(usage_lines)
                .block(inner_block("USAGE"))
                .wrap(Wrap { trim: false });
            frame.render_widget(usage_panel, body_cols[0]);

            // right column: OPTIONS
            fn opt_line<'a>(flag: &'a str, desc: &'a str) -> Line<'a> {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<18}", flag),
                        Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(desc, Style::default().fg(ON_SURFACE)),
                ])
            }

            let opts_lines = vec![
                Line::raw(""),
                opt_line("-s, --search", "Search query instead of clean"),
                Line::raw(""),
                opt_line("-v, --verbose", "Both clean error + search query"),
                Line::raw(""),
                opt_line("-r, --raw", "Skip heuristic pre-filter"),
                Line::raw(""),
                opt_line("-n, --no-model", "Pre-filter only, no model"),
                Line::raw(""),
                opt_line("    --no-copy", "Don't copy output to clipboard"),
                Line::raw(""),
                opt_line("    --download", "Force re-download the model"),
                Line::raw(""),
                opt_line("-h, --help", "Print help"),
                Line::raw(""),
                opt_line("-V, --version", "Print version"),
                Line::raw(""),
            ];
            let opts_panel = Paragraph::new(opts_lines)
                .block(inner_block("OPTIONS"))
                .wrap(Wrap { trim: false });
            frame.render_widget(opts_panel, body_cols[2]);

            // ── status bar ───────────────────────────────────────────────
            let status = Paragraph::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    spin_char.to_string(),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  Ready  ", Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled("│  ", Style::default().fg(MUTED)),
                Span::styled("[q] ", Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD)),
                Span::styled("quit  ", Style::default().fg(ON_SURFACE)),
                Span::styled("[any key] ", Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD)),
                Span::styled("exit  ", Style::default().fg(ON_SURFACE)),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(MUTED))
                    .style(Style::default().bg(SURFACE)),
            )
            .alignment(Alignment::Left);
            frame.render_widget(status, root[2]);

            // ── decorative: version badge in top-right of header ─────────
            let badge_area = Rect {
                x: root[0].right().saturating_sub(16),
                y: root[0].y,
                width: 14,
                height: 1,
            };
            // only draw if there's room
            let version = format!(" v{} ", env!("CARGO_PKG_VERSION"));
            if root[0].width > 40 {
                let badge = Paragraph::new(Span::styled(
                    version.as_str(),
                    Style::default()
                        .fg(Color::Black)
                        .bg(ACCENT2)
                        .add_modifier(Modifier::BOLD),
                ));
                frame.render_widget(badge, badge_area);
            }
        })?;

        frame_idx += 1;

        // Poll for key with timeout so the spinner can tick
        if event::poll(Duration::from_millis(120))? {
            if let Event::Key(key) = event::read()? {
                // q, Esc, Enter, Ctrl-C all quit
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    _ => break, // any other key too
                }
            }
        }
    }

    Ok(())
}

// ── loading spinner ──────────────────────────────────────────────────────────

/// Handle for a running loading spinner. Drop or call stop() to stop it.
pub struct LoadingHandle {
    stop_flag: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl LoadingHandle {
    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for LoadingHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            handle.join().ok();
        }
    }
}

/// Show a non-blocking spinner on stderr. Returns a handle to stop it.
pub fn show_loading_spinner(msg: &str) -> LoadingHandle {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = stop_flag.clone();
    let msg = msg.to_string();

    let thread = thread::spawn(move || {
        let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let mut i = 0;

        let mut writer = io::stderr();

        while !flag_clone.load(Ordering::Acquire) {
            write!(
                writer,
                "\r\x1b[36m{}\x1b[0m \x1b[1m{}\x1b[0m  ",
                frames[i % frames.len()],
                msg
            )
            .ok();
            writer.flush().ok();
            i += 1;
            thread::sleep(Duration::from_millis(80));
        }
        // Clear the spinner line
        write!(writer, "\r{}\r", " ".repeat(msg.len() + 10)).ok();
        writer.flush().ok();
    });

    LoadingHandle {
        stop_flag,
        thread: Some(thread),
    }
}
