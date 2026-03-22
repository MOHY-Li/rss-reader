mod config;
mod fetcher;
mod models;
mod store;
mod tui;

use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::io;

use config::Args;
use feedparser_rs::{Entry, ParsedFeed};
use models::{AppState, EntryView, FeedState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = Args::parse();
    let feeds = match args.resolve_feeds() {
        Ok(feeds) => feeds,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    let state_path = store::default_state_path();
    let mut app_state = store::load_state(&state_path);
    let seen_cap = args.seen_cap.max(1);

    for feed_url in &feeds {
        ensure_feed_state(&mut app_state, feed_url, seen_cap);
    }

    let client = fetcher::build_client(&args)?;

    let refresh_client = client.clone();
    let refresh_feeds = feeds.clone();
    let refresh_state_path = state_path.clone();
    let refresh_seen_cap = seen_cap;
    let refresh_max_items = args.max_items;
    let handle = tokio::runtime::Handle::current();
    let refresh = move |app: &mut AppState| -> io::Result<()> {
        let refresh_result = tokio::task::block_in_place(|| {
            handle.block_on(refresh_all_feeds(
                app,
                &refresh_client,
                &refresh_feeds,
                refresh_seen_cap,
                refresh_max_items,
            ))
        });

        if refresh_result.changed {
            store::save_state(&refresh_state_path, app)
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        }

        if let Some(err) = refresh_result.error {
            return Err(err);
        }

        Ok(())
    };

    tui::run_tui(&mut app_state, refresh)?;
    store::save_state(&state_path, &app_state)?;
    Ok(())
}

struct RefreshOutcome {
    changed: bool,
    error: Option<io::Error>,
}

async fn refresh_all_feeds(
    app: &mut AppState,
    client: &reqwest::Client,
    feeds: &[String],
    seen_cap: usize,
    max_items: usize,
) -> RefreshOutcome {
    let mut changed = false;
    let mut last_error: Option<io::Error> = None;

    for feed_url in feeds {
        match fetcher::fetch_and_parse(client, feed_url).await {
            Ok(feed) => {
                ensure_feed_state(app, feed_url, seen_cap);
                if let Some(feed_state) = app.feeds.get_mut(feed_url) {
                    let (_new_entries, updated, _label) =
                        apply_feed_update(feed_state, &feed, seen_cap, max_items);
                    if updated {
                        changed = true;
                    }
                }
            }
            Err(err) => {
                last_error = Some(io::Error::new(io::ErrorKind::Other, err));
            }
        }
    }

    RefreshOutcome {
        changed,
        error: last_error,
    }
}

fn ensure_feed_state(app: &mut AppState, feed_url: &str, seen_cap: usize) {
    let feed_state = app.feeds.entry(feed_url.to_string()).or_insert_with(|| {
        FeedState {
            feed_url: feed_url.to_string(),
            title: None,
            entries: Vec::new(),
            seen_keys: Vec::new(),
            read_keys: Vec::new(),
        }
    });

    if feed_state.feed_url != feed_url {
        feed_state.feed_url = feed_url.to_string();
    }

    if feed_state.seen_keys.is_empty() && !feed_state.entries.is_empty() {
        feed_state.seen_keys = feed_state
            .entries
            .iter()
            .map(|entry| entry.key.clone())
            .collect();
    }

    if feed_state.seen_keys.len() > seen_cap {
        let excess = feed_state.seen_keys.len() - seen_cap;
        feed_state.seen_keys.drain(0..excess);
    }
}

fn apply_feed_update(
    feed_state: &mut FeedState,
    feed: &ParsedFeed,
    seen_cap: usize,
    max_items: usize,
) -> (Vec<Entry>, bool, String) {
    let mut seen_set: HashSet<String> = feed_state.seen_keys.iter().cloned().collect();
    let mut seen_order: VecDeque<String> = feed_state.seen_keys.iter().cloned().collect();
    while seen_order.len() > seen_cap {
        if let Some(old) = seen_order.pop_front() {
            seen_set.remove(&old);
        }
    }

    let mut new_entries = Vec::new();
    let mut new_views = Vec::new();
    let mut new_keys = HashSet::new();

    for (index, entry) in feed.entries.iter().enumerate() {
        let key = entry_key(entry, index);
        if seen_set.contains(&key) {
            continue;
        }

        remember_entry(key.clone(), &mut seen_set, &mut seen_order, seen_cap);
        new_entries.push(entry.clone());
        new_views.push(EntryView {
            key: key.clone(),
            title: entry_title(entry),
            link: entry_link(entry),
            is_read: false,
        });
        new_keys.insert(key);

        if new_entries.len() >= max_items {
            break;
        }
    }

    let mut changed = false;
    if !new_views.is_empty() {
        let mut combined = new_views;
        combined.extend(feed_state.entries.drain(..));
        feed_state.entries = combined;
        feed_state.read_keys.retain(|key| !new_keys.contains(key));
        changed = true;
    }

    let next_title = feed.feed.title.as_deref().map(|title| title.to_string());
    if feed_state.title != next_title {
        feed_state.title = next_title;
        changed = true;
    }

    feed_state.seen_keys = seen_order.into_iter().collect();
    let label = feed
        .feed
        .title
        .as_deref()
        .unwrap_or(&feed_state.feed_url)
        .to_string();

    (new_entries, changed, label)
}

fn remember_entry(
    key: String,
    seen: &mut HashSet<String>,
    seen_order: &mut VecDeque<String>,
    seen_cap: usize,
) {
    if seen.insert(key.clone()) {
        seen_order.push_back(key);
        while seen_order.len() > seen_cap {
            if let Some(old) = seen_order.pop_front() {
                seen.remove(&old);
            }
        }
    }
}

fn entry_key(entry: &Entry, index: usize) -> String {
    if let Some(id) = entry.id.as_deref() {
        return format!("id:{id}");
    }
    if let Some(link) = entry.link.as_deref() {
        return format!("link:{link}");
    }
    if let Some(link) = entry.links.first().map(|link| link.href.as_str()) {
        return format!("link:{link}");
    }
    if let Some(title) = entry.title.as_deref() {
        if let Some(published) = entry.published {
            return format!("title:{title}|published:{}", published.to_rfc3339());
        }
        return format!("title:{title}");
    }

    let summary = entry.summary.as_deref().unwrap_or("");
    let author = entry.author.as_deref().unwrap_or("");
    let published = entry
        .published
        .map(|value| value.to_rfc3339())
        .unwrap_or_default();
    if summary.is_empty() && author.is_empty() && published.is_empty() {
        return format!("fallback:empty:{index}");
    }
    format!("fallback:{summary}|{author}|{published}")
}

fn entry_title(entry: &Entry) -> String {
    entry
        .title
        .clone()
        .unwrap_or_else(|| "(untitled)".to_string())
}

fn entry_link(entry: &Entry) -> String {
    entry
        .link
        .as_deref()
        .or_else(|| entry.links.first().map(|link| link.href.as_str()))
        .unwrap_or("")
        .to_string()
}
