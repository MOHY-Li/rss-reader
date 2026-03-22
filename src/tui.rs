use crate::models::{AppState, EntryView, FeedState};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Feeds,
    Entries,
}

#[derive(Debug, Clone)]
struct TuiState {
    feed_index: usize,
    entry_index: usize,
    focus: Focus,
    status: String,
}

impl TuiState {
    fn new() -> Self {
        Self {
            feed_index: 0,
            entry_index: 0,
            focus: Focus::Feeds,
            status: String::from(
                "q/Esc quit • Tab switch • j/k or ↑/↓ move • Enter open • r refresh",
            ),
        }
    }
}

pub fn run_tui<F>(app: &mut AppState, mut refresh: F) -> io::Result<()>
where
    F: FnMut(&mut AppState) -> io::Result<()>,
{
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut state = TuiState::new();
    match refresh(app) {
        Ok(()) => {
            state.status = String::from("Refresh complete");
        }
        Err(err) => {
            state.status = format!("Refresh failed: {}", err);
        }
    }
    let result = run_loop(&mut terminal, app, &mut state, &mut refresh);

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

fn run_loop<F>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
    state: &mut TuiState,
    refresh: &mut F,
) -> io::Result<()>
where
    F: FnMut(&mut AppState) -> io::Result<()>,
{
    loop {
        normalize_state(app, state);
        terminal.draw(|frame| draw_ui(frame, app, state))?;

        match event::read()? {
            Event::Key(key) => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Tab => {
                    state.focus = match state.focus {
                        Focus::Feeds => Focus::Entries,
                        Focus::Entries => Focus::Feeds,
                    };
                }
                KeyCode::Char('j') | KeyCode::Down => match state.focus {
                    Focus::Feeds => {
                        let feeds = collect_feeds(app);
                        move_selection(&mut state.feed_index, feeds.len(), 1);
                        state.entry_index = 0;
                    }
                    Focus::Entries => {
                        let feeds = collect_feeds(app);
                        let entries = selected_entries(&feeds, state.feed_index);
                        move_selection(&mut state.entry_index, entries.len(), 1);
                    }
                },
                KeyCode::Char('k') | KeyCode::Up => match state.focus {
                    Focus::Feeds => {
                        let feeds = collect_feeds(app);
                        move_selection(&mut state.feed_index, feeds.len(), -1);
                        state.entry_index = 0;
                    }
                    Focus::Entries => {
                        let feeds = collect_feeds(app);
                        let entries = selected_entries(&feeds, state.feed_index);
                        move_selection(&mut state.entry_index, entries.len(), -1);
                    }
                },
                KeyCode::Enter => {
                    let selection = {
                        let feeds = collect_feeds(app);
                        feeds.get(state.feed_index).and_then(|feed_ref| {
                            feed_ref.feed.entries.get(state.entry_index).map(|entry| {
                                (
                                    feed_ref.key.clone(),
                                    entry.link.clone(),
                                    entry.title.clone(),
                                )
                            })
                        })
                    };

                    if let Some((feed_key, link, title)) = selection {
                        let open_result = open::that(&link);
                        if let Some(feed) = app.feeds.get_mut(&feed_key) {
                            if let Some(entry) = feed.entries.get_mut(state.entry_index) {
                                entry.is_read = true;
                            }
                        }
                        match open_result {
                            Ok(_) => {
                                state.status = format!("Opened: {}", title);
                            }
                            Err(err) => {
                                state.status = format!("Failed to open link: {}", err);
                            }
                        }
                    } else {
                        state.status = String::from("No entry selected");
                    }
                }
                KeyCode::Char('r') => match refresh(app) {
                    Ok(()) => {
                        state.status = String::from("Refresh complete");
                    }
                    Err(err) => {
                        state.status = format!("Refresh failed: {}", err);
                    }
                },
                _ => {}
            },
            _ => {}
        }
    }

    Ok(())
}

fn normalize_state(app: &AppState, state: &mut TuiState) {
    let feeds = collect_feeds(app);
    normalize_selection(&mut state.feed_index, feeds.len());
    let entries = selected_entries(&feeds, state.feed_index);
    normalize_selection(&mut state.entry_index, entries.len());
}

fn normalize_selection(index: &mut usize, len: usize) -> Option<usize> {
    if len == 0 {
        *index = 0;
        None
    } else {
        if *index >= len {
            *index = len.saturating_sub(1);
        }
        Some(*index)
    }
}

fn move_selection(index: &mut usize, len: usize, delta: isize) {
    if len == 0 {
        *index = 0;
        return;
    }
    let max = len.saturating_sub(1) as isize;
    let next = (*index as isize + delta).clamp(0, max);
    *index = next as usize;
}

struct FeedRef<'a> {
    key: &'a String,
    feed: &'a FeedState,
}

fn collect_feeds(app: &AppState) -> Vec<FeedRef<'_>> {
    let mut feeds: Vec<FeedRef<'_>> = app
        .feeds
        .iter()
        .map(|(key, feed)| FeedRef { key, feed })
        .collect();
    feeds.sort_by_key(|feed_ref| feed_label(feed_ref.feed).to_lowercase());
    feeds
}

fn feed_label(feed: &FeedState) -> String {
    feed.title.clone().unwrap_or_else(|| feed.feed_url.clone())
}

fn selected_entries<'a>(feeds: &[FeedRef<'a>], index: usize) -> &'a [EntryView] {
    feeds
        .get(index)
        .map(|feed_ref| feed_ref.feed.entries.as_slice())
        .unwrap_or(&[])
}

fn draw_ui(frame: &mut Frame, app: &AppState, state: &mut TuiState) {
    let feeds = collect_feeds(app);
    let feed_selected = normalize_selection(&mut state.feed_index, feeds.len());
    let entries = selected_entries(&feeds, state.feed_index);
    let entry_selected = normalize_selection(&mut state.entry_index, entries.len());

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let title = Paragraph::new(Line::from(vec![
        Span::styled("RSS Reader", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  Feeds: {}", feeds.len())),
        Span::raw(format!("  Entries: {}", entries.len())),
    ]));
    frame.render_widget(title, layout[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(layout[1]);

    let feed_items: Vec<ListItem> = feeds
        .iter()
        .map(|feed_ref| {
            ListItem::new(format!(
                "{} ({})",
                feed_label(feed_ref.feed),
                feed_ref.feed.entries.len()
            ))
        })
        .collect();

    let entry_items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let prefix = if entry.is_read { "✓" } else { "•" };
            ListItem::new(format!("{} {}", prefix, entry.title))
        })
        .collect();

    let focus_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().add_modifier(Modifier::DIM);

    let feed_list = List::new(feed_items)
        .block(Block::default().borders(Borders::ALL).title("Feeds"))
        .highlight_symbol("> ")
        .highlight_style(if state.focus == Focus::Feeds {
            focus_style
        } else {
            inactive_style
        });

    let entry_list = List::new(entry_items)
        .block(Block::default().borders(Borders::ALL).title("Entries"))
        .highlight_symbol("> ")
        .highlight_style(if state.focus == Focus::Entries {
            focus_style
        } else {
            inactive_style
        });

    let mut feed_state = ListState::default();
    feed_state.select(feed_selected);
    frame.render_stateful_widget(feed_list, body[0], &mut feed_state);

    let mut entry_state = ListState::default();
    entry_state.select(entry_selected);
    frame.render_stateful_widget(entry_list, body[1], &mut entry_state);

    let status = Paragraph::new(Line::from(Span::raw(state.status.clone())))
        .style(Style::default().fg(Color::LightCyan));
    frame.render_widget(status, layout[2]);
}
