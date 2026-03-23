use crate::config;
use crate::models::{AppState, EntryView, FeedState};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use open;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Feeds,
    Entries,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortMode {
    NewFirst,
    OldFirst,
    UnreadOnly,
    ReadOnly,
}

impl SortMode {
    fn next(self) -> Self {
        match self {
            SortMode::NewFirst => SortMode::OldFirst,
            SortMode::OldFirst => SortMode::UnreadOnly,
            SortMode::UnreadOnly => SortMode::ReadOnly,
            SortMode::ReadOnly => SortMode::NewFirst,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SortMode::NewFirst => "Sort: new→old",
            SortMode::OldFirst => "Sort: old→new",
            SortMode::UnreadOnly => "Filter: unread",
            SortMode::ReadOnly => "Filter: read",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Search,
    AddFeed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalMode {
    ConfirmDelete,
    Input,
}

#[derive(Debug, Clone)]
struct TuiState {
    feed_index: usize,
    entry_index: usize,
    focus: Focus,
    help: String,
    notice: Option<String>,
    search_query: String,
    input_mode: Option<InputMode>,
    input_buffer: String,
    modal_mode: Option<ModalMode>,
    pending_delete: Option<String>,
    sort_mode: SortMode,
}

impl TuiState {
    fn new() -> Self {
        Self {
            feed_index: 0,
            entry_index: 0,
            focus: Focus::Feeds,
            help: String::from(
                "q quit • Esc/← back • Enter/→ enter • j/k move • / search • s sort • a add/import • d delete • r refresh",
            ),
            notice: None,
            search_query: String::new(),
            input_mode: None,
            input_buffer: String::new(),
            modal_mode: None,
            pending_delete: None,
            sort_mode: SortMode::NewFirst,
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
    if let Err(err) = refresh(app) {
        state.notice = Some(format!("Refresh failed: {}", err));
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
            Event::Key(key) => {
                if key.code == KeyCode::Char('q') {
                    break;
                }

                if state.modal_mode == Some(ModalMode::ConfirmDelete) {
                    handle_confirm_key(app, state, key.code);
                    continue;
                }

                if state.input_mode.is_some() {
                    handle_input_key(app, state, refresh, key.code);
                    continue;
                }

                match key.code {
                    KeyCode::Esc => {
                        if state.focus == Focus::Entries {
                            state.focus = Focus::Feeds;
                            state.entry_index = 0;
                        }
                    }
                    KeyCode::Left => {
                        if state.focus == Focus::Entries {
                            state.focus = Focus::Feeds;
                            state.entry_index = 0;
                        }
                    }
                    KeyCode::Char('/') => {
                        state.input_mode = Some(InputMode::Search);
                        state.input_buffer.clear();
                        state.search_query.clear();
                        state.entry_index = 0;
                        state.modal_mode = Some(ModalMode::Input);
                    }
                    KeyCode::Char('s') => {
                        state.sort_mode = state.sort_mode.next();
                        state.entry_index = 0;
                    }
                    KeyCode::Char('a') => {
                        state.input_mode = Some(InputMode::AddFeed);
                        state.input_buffer.clear();
                        state.modal_mode = Some(ModalMode::Input);
                    }
                    KeyCode::Char('d') => {
                        if state.focus == Focus::Feeds {
                            request_delete_selected_feed(app, state);
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Down => match state.focus {
                        Focus::Feeds => {
                            let feeds = collect_feeds(app);
                            move_selection(&mut state.feed_index, feeds.len(), 1);
                            state.entry_index = 0;
                        }
                        Focus::Entries => {
                            let feeds = collect_feeds(app);
                            let entries = filtered_entries(&feeds, state.feed_index, state);
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
                            let entries = filtered_entries(&feeds, state.feed_index, state);
                            move_selection(&mut state.entry_index, entries.len(), -1);
                        }
                    },
                    KeyCode::Enter | KeyCode::Right => {
                        if state.focus == Focus::Feeds {
                            state.focus = Focus::Entries;
                            state.entry_index = 0;
                            continue;
                        }
                        if state.focus == Focus::Entries {
                            let selection = {
                                let feeds = collect_feeds(app);
                                feeds.get(state.feed_index).and_then(|feed_ref| {
                                    let entries = filtered_entries(&feeds, state.feed_index, state);
                                    entries.get(state.entry_index).map(|entry_ref| {
                                        (
                                            feed_ref.key.clone(),
                                            entry_ref.entry.key.clone(),
                                            entry_ref.entry.link.clone(),
                                        )
                                    })
                                })
                            };

                            if let Some((feed_key, entry_key, link)) = selection {
                                mark_entry_read(app, &feed_key, &entry_key);
                                if link.trim().is_empty() {
                                    state.notice = Some(String::from("No link to open"));
                                } else if let Err(err) = open::that(&link) {
                                    state.notice = Some(format!("Open failed: {}", err));
                                } else {
                                    state.notice = Some(String::from("Opened in browser"));
                                }
                            } else {
                                state.notice = Some(String::from("No entry selected"));
                            }
                            continue;
                        }
                    }
                    KeyCode::Char('r') => match refresh(app) {
                        Ok(()) => state.notice = Some(String::from("Refresh complete")),
                        Err(err) => state.notice = Some(format!("Refresh failed: {}", err)),
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn normalize_state(app: &AppState, state: &mut TuiState) {
    let feeds = collect_feeds(app);
    normalize_selection(&mut state.feed_index, feeds.len());
    let entries = filtered_entries(&feeds, state.feed_index, state);
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

fn request_delete_selected_feed(app: &AppState, state: &mut TuiState) {
    let feeds = collect_feeds(app);
    let Some(feed_ref) = feeds.get(state.feed_index) else {
        state.notice = Some(String::from("No feed selected"));
        return;
    };
    state.pending_delete = Some(feed_ref.key.clone());
    state.modal_mode = Some(ModalMode::ConfirmDelete);
}

fn confirm_delete_selected_feed(app: &mut AppState, state: &mut TuiState) {
    let Some(feed_key) = state.pending_delete.take() else {
        state.modal_mode = None;
        return;
    };

    if app.feeds.remove(&feed_key).is_some() {
        state.notice = Some(String::from("Feed deleted"));
        state.entry_index = 0;
        if state.feed_index > 0 {
            state.feed_index -= 1;
        }
        if app.feeds.is_empty() {
            state.feed_index = 0;
            state.focus = Focus::Feeds;
        }
    } else {
        state.notice = Some(String::from("Feed not found"));
    }

    state.modal_mode = None;
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

struct EntryRef<'a> {
    entry: &'a EntryView,
}

fn filtered_entries<'a>(
    feeds: &[FeedRef<'a>],
    index: usize,
    state: &TuiState,
) -> Vec<EntryRef<'a>> {
    let entries = feeds
        .get(index)
        .map(|feed_ref| feed_ref.feed.entries.as_slice())
        .unwrap_or(&[]);

    let query = state.search_query.trim().to_lowercase();
    let mut filtered: Vec<EntryRef<'a>> = entries
        .iter()
        .filter(|entry| {
            match state.sort_mode {
                SortMode::UnreadOnly if entry.is_read => return false,
                SortMode::ReadOnly if !entry.is_read => return false,
                _ => {}
            }
            if query.is_empty() {
                return true;
            }
            let title = entry.title.to_lowercase();
            let link = entry.link.to_lowercase();
            title.contains(&query) || link.contains(&query)
        })
        .map(|entry| EntryRef { entry })
        .collect();

    filtered.sort_by(|a, b| {
        let left = a.entry.published.as_deref().unwrap_or("");
        let right = b.entry.published.as_deref().unwrap_or("");
        match state.sort_mode {
            SortMode::OldFirst => left.cmp(right),
            _ => right.cmp(left),
        }
    });
    filtered
}

fn draw_ui(frame: &mut Frame, app: &AppState, state: &mut TuiState) {
    let feeds = collect_feeds(app);
    let feed_selected = normalize_selection(&mut state.feed_index, feeds.len());
    let entries = filtered_entries(&feeds, state.feed_index, state);
    let entry_selected = normalize_selection(&mut state.entry_index, entries.len());
    let total_entries = feeds
        .get(state.feed_index)
        .map(|feed_ref| feed_ref.feed.entries.len())
        .unwrap_or(0);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let search_label = if state.search_query.is_empty() {
        String::from("Search: (none)")
    } else {
        format!("Search: {}", state.search_query)
    };
    let input_label = match state.input_mode {
        Some(InputMode::Search) => format!("Search: {}", state.input_buffer),
        Some(InputMode::AddFeed) => format!("Add feed: {}", state.input_buffer),
        None => search_label,
    };
    let sort_label = state.sort_mode.label();
    let title = Paragraph::new(Line::from(vec![
        Span::styled("RSS Reader", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  Feeds: {}", feeds.len())),
        Span::raw(format!("  Entries: {}/{}", entries.len(), total_entries)),
        Span::raw(format!("  {}", sort_label)),
        Span::raw(format!("  {}", input_label)),
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
        .map(|entry_ref| {
            let entry = entry_ref.entry;
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

    let help_text = if let Some(notice) = &state.notice {
        format!("{}  |  {}", state.help, notice)
    } else {
        state.help.clone()
    };
    let help = Paragraph::new(Line::from(Span::raw(help_text)))
        .style(Style::default().fg(Color::LightCyan));
    frame.render_widget(help, layout[2]);

    if let Some(modal) = state.modal_mode {
        draw_modal(frame, state, modal);
    }
}

fn draw_modal(frame: &mut Frame, state: &TuiState, modal: ModalMode) {
    let area = centered_rect(60, 30, frame.area());
    frame.render_widget(Clear, area);

    let title = match modal {
        ModalMode::ConfirmDelete => "Confirm delete",
        ModalMode::Input => match state.input_mode {
            Some(InputMode::Search) => "Search",
            Some(InputMode::AddFeed) => "Add feed",
            None => "Input",
        },
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    match modal {
        ModalMode::ConfirmDelete => {
            let feed_label = state.pending_delete.as_deref().unwrap_or("selected feed");
            let lines = vec![
                Line::from(format!("Delete {}?", feed_label)),
                Line::from("Press y to confirm, n or Esc to cancel"),
            ];
            frame.render_widget(Paragraph::new(lines), inner);
        }
        ModalMode::Input => {
            let (prompt, hint) = match state.input_mode {
                Some(InputMode::Search) => ("Search query:", "Enter to apply, Esc to cancel"),
                Some(InputMode::AddFeed) => (
                    "Feed URL or file path:",
                    "Enter to add/import, Esc to cancel",
                ),
                None => ("", ""),
            };
            let lines = vec![
                Line::from(prompt),
                Line::from(state.input_buffer.clone()),
                Line::from(hint),
            ];
            frame.render_widget(Paragraph::new(lines), inner);

            let cursor_x = inner
                .x
                .saturating_add(state.input_buffer.chars().count() as u16);
            let cursor_y = inner.y.saturating_add(1);
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}

fn mark_entry_read(app: &mut AppState, feed_key: &str, entry_key: &str) {
    if let Some(feed) = app.feeds.get_mut(feed_key) {
        if let Some(entry) = feed.entries.iter_mut().find(|entry| entry.key == entry_key) {
            entry.is_read = true;
        }
    }
}

fn exit_input_mode(state: &mut TuiState, clear_search: bool) {
    if state.input_mode == Some(InputMode::Search) && clear_search {
        state.search_query.clear();
        state.input_buffer.clear();
    }
    state.input_mode = None;
    if state.modal_mode == Some(ModalMode::Input) {
        state.modal_mode = None;
    }
    if state.notice.is_some() {
        state.notice = None;
    }
}

fn handle_input_key<F>(app: &mut AppState, state: &mut TuiState, refresh: &mut F, key: KeyCode)
where
    F: FnMut(&mut AppState) -> io::Result<()>,
{
    match key {
        KeyCode::Esc => exit_input_mode(state, true),
        KeyCode::Backspace => {
            state.input_buffer.pop();
            if state.input_mode == Some(InputMode::Search) {
                state.search_query = state.input_buffer.clone();
                state.entry_index = 0;
            }
        }
        KeyCode::Enter => match state.input_mode {
            Some(InputMode::Search) => {
                exit_input_mode(state, false);
                state.entry_index = 0;
            }
            Some(InputMode::AddFeed) => handle_add_feed(app, state, refresh),
            None => {}
        },
        KeyCode::Char(ch) => {
            state.input_buffer.push(ch);
            if state.input_mode == Some(InputMode::Search) {
                state.search_query = state.input_buffer.clone();
                state.entry_index = 0;
            }
        }
        _ => {}
    }
}

fn handle_confirm_key(app: &mut AppState, state: &mut TuiState, key: KeyCode) {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => confirm_delete_selected_feed(app, state),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            state.pending_delete = None;
            state.modal_mode = None;
        }
        _ => {}
    }
}

fn handle_add_feed<F>(app: &mut AppState, state: &mut TuiState, refresh: &mut F)
where
    F: FnMut(&mut AppState) -> io::Result<()>,
{
    let raw = state.input_buffer.trim();
    if raw.is_empty() {
        state.notice = Some(String::from("Feed URL or file path is empty"));
        return;
    }

    let path = PathBuf::from(raw);
    if path.is_file() {
        match config::parse_feeds_file(&path) {
            Ok(feeds) => {
                if feeds.is_empty() {
                    state.notice = Some(String::from("No feeds found in file"));
                    return;
                }

                let mut first_added: Option<String> = None;
                let mut added_count = 0;
                for feed in feeds {
                    if insert_feed(app, &feed) {
                        if first_added.is_none() {
                            first_added = Some(feed.clone());
                        }
                        added_count += 1;
                    }
                }

                if added_count == 0 {
                    state.notice = Some(String::from("Feeds already exist"));
                } else {
                    if let Err(err) = refresh(app) {
                        state.notice = Some(format!("Refresh failed: {}", err));
                    } else {
                        state.notice = Some(format!("Imported {} feeds", added_count));
                    }
                    if let Some(feed) = first_added {
                        select_feed(state, app, &feed);
                    }
                }
                state.input_buffer.clear();
                state.input_mode = None;
                state.modal_mode = None;
            }
            Err(err) => {
                state.notice = Some(err.to_string());
            }
        }
        return;
    }

    match config::validate_feed_url(raw, None) {
        Ok(validated) => {
            let added = insert_feed(app, &validated);
            if added {
                if let Err(err) = refresh(app) {
                    state.notice = Some(format!("Refresh failed: {}", err));
                } else {
                    state.notice = Some(String::from("Feed added"));
                }
                select_feed(state, app, &validated);
            } else {
                state.notice = Some(String::from("Feed already exists"));
            }
            state.input_buffer.clear();
            state.input_mode = None;
            state.modal_mode = None;
        }
        Err(err) => {
            state.notice = Some(err.to_string());
        }
    }
}

fn insert_feed(app: &mut AppState, feed_url: &str) -> bool {
    if app.feeds.contains_key(feed_url) {
        return false;
    }

    app.feeds.insert(
        feed_url.to_string(),
        FeedState {
            feed_url: feed_url.to_string(),
            title: None,
            entries: Vec::new(),
            seen_keys: Vec::new(),
            read_keys: Vec::new(),
        },
    );
    true
}

fn select_feed(state: &mut TuiState, app: &AppState, feed_url: &str) {
    let feeds = collect_feeds(app);
    if let Some(index) = feeds.iter().position(|feed_ref| feed_ref.key == feed_url) {
        state.feed_index = index;
    }
}
