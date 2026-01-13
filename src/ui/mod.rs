//! TUI (Terminal User Interface) module using ratatui

mod state;
mod widgets;

use crate::manager::{ManagerState, Metrics};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub use state::AppState;

/// Initialize the terminal for TUI
pub fn init_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore the terminal to its original state
pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Main UI drawing function
pub fn draw(frame: &mut Frame, app_state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(7),  // Header & Status
            Constraint::Length(5),  // Hashrate
            Constraint::Length(5),  // Shares
            Constraint::Min(10),    // Logs
            Constraint::Length(3),  // Footer
        ])
        .split(frame.area());

    draw_header(frame, chunks[0], app_state);
    draw_hashrate(frame, chunks[1], app_state);
    draw_shares(frame, chunks[2], app_state);
    draw_logs(frame, chunks[3], app_state);
    draw_footer(frame, chunks[4]);
}

fn draw_header(frame: &mut Frame, area: Rect, app_state: &AppState) {
    let status_color = if app_state.connected {
        Color::Green
    } else {
        Color::Red
    };

    let status_text = if app_state.connected {
        "● Connected"
    } else {
        "○ Disconnected"
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                "╔═══════════════════════════════════════════════════════════╗",
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("║ ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "⛏  LightMiner-Rust",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("                                        "),
            Span::styled("║", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled(
                "╚═══════════════════════════════════════════════════════════╝",
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  Pool: "),
            Span::styled(&app_state.pool_address, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::raw("  Status: "),
            Span::styled(status_text, Style::default().fg(status_color)),
            Span::raw("    Difficulty: "),
            Span::styled(
                format!("{:.4}", app_state.difficulty),
                Style::default().fg(Color::Magenta),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn draw_hashrate(frame: &mut Frame, area: Rect, app_state: &AppState) {
    let hashrate = app_state.hashrate_formatted();
    let hashes = app_state.total_hashes;

    let block = Block::default()
        .title(" ⚡ Hashrate ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = vec![
        Line::from(vec![
            Span::raw("  Current: "),
            Span::styled(
                &hashrate,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  Total Hashes: "),
            Span::styled(
                format!("{}", hashes),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(text);
    frame.render_widget(paragraph, inner);
}

fn draw_shares(frame: &mut Frame, area: Rect, app_state: &AppState) {
    let block = Block::default()
        .title(" 📊 Shares ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let total = app_state.accepted + app_state.rejected;
    let ratio = if total > 0 {
        (app_state.accepted as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let text = vec![
        Line::from(vec![
            Span::raw("  Accepted: "),
            Span::styled(
                format!("{}", app_state.accepted),
                Style::default().fg(Color::Green),
            ),
            Span::raw("    Rejected: "),
            Span::styled(
                format!("{}", app_state.rejected),
                Style::default().fg(Color::Red),
            ),
            Span::raw("    Ratio: "),
            Span::styled(
                format!("{:.1}%", ratio),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(text);
    frame.render_widget(paragraph, inner);
}

fn draw_logs(frame: &mut Frame, area: Rect, app_state: &AppState) {
    let block = Block::default()
        .title(" 📜 Recent Logs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let items: Vec<ListItem> = app_state
        .logs
        .iter()
        .rev()
        .take(20)
        .map(|log| {
            let style = if log.contains("error") || log.contains("Error") {
                Style::default().fg(Color::Red)
            } else if log.contains("accepted") || log.contains("Found") {
                Style::default().fg(Color::Green)
            } else if log.contains("job") || log.contains("Job") {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Span::styled(log.clone(), style))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled(" [Q] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Quit  "),
        Span::styled(" [C] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Clear Logs  "),
    ]);

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(paragraph, area);
}

/// Run the TUI event loop
pub async fn run_ui(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
    app_state: Arc<tokio::sync::RwLock<AppState>>,
    mut shutdown_rx: mpsc::Receiver<()>,
) -> Result<()> {
    loop {
        // Draw UI
        {
            let state = app_state.read().await;
            terminal.draw(|f| draw(f, &state))?;
        }

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            break;
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            let mut state = app_state.write().await;
                            state.logs.clear();
                        }
                        _ => {}
                    }
                }
            }
        }

        // Check for shutdown signal
        if shutdown_rx.try_recv().is_ok() {
            break;
        }
    }

    restore_terminal(&mut terminal)?;
    Ok(())
}
