#[path = "../src/config.rs"]
mod config;
#[path = "../src/models.rs"]
mod models;
#[path = "../src/store.rs"]
mod store;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use config::{Args, FeedResolveError};
use models::{AppState, EntryView, FeedState};

fn temp_path(label: &str, extension: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time available")
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("rss_reader_{label}_{pid}_{nanos}.{extension}"));
    path
}

fn base_args() -> Args {
    Args {
        url: Some("https://example.com".to_string()),
        feeds: Vec::new(),
        feeds_file: None,
        timeout_secs: 20,
        user_agent: "rss-reader/0.1".to_string(),
        max_items: 50,
        seen_cap: 2000,
    }
}

#[test]
fn resolve_feeds_file_ignores_comments_and_blank_lines() {
    let path = temp_path("feeds", "txt");
    let contents = "\n# comment\n https://example.com/feed \n\nhttps://example.com/other\n   # trailing comment\n";
    fs::write(&path, contents).expect("write feeds file");

    let mut args = base_args();
    args.feeds_file = Some(path.clone());
    let feeds = args.resolve_feeds().expect("resolve feeds");

    assert_eq!(
        feeds,
        vec![
            "https://example.com/feed".to_string(),
            "https://example.com/other".to_string()
        ]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolve_feeds_file_reports_invalid_url_line_number() {
    let path = temp_path("feeds_invalid", "txt");
    let contents = "https://example.com/feed\n\nnot a url\n";
    fs::write(&path, contents).expect("write feeds file");

    let mut args = base_args();
    args.feeds_file = Some(path.clone());
    let err = args.resolve_feeds().expect_err("invalid URL should error");

    let message = format!("{err}");
    match &err {
        FeedResolveError::InvalidUrl { line, value } => {
            assert_eq!(*line, Some(3));
            assert_eq!(value, "not a url");
            assert!(message.contains("line 3"));
        }
        other => panic!("unexpected error: {other}"),
    }

    let _ = fs::remove_file(path);
}

#[test]
fn save_and_load_state_round_trip() {
    let path = temp_path("state_round_trip", "json");
    let mut feeds = HashMap::new();
    feeds.insert(
        "https://example.com/feed".to_string(),
        FeedState {
            feed_url: "https://example.com/feed".to_string(),
            title: Some("Example Feed".to_string()),
            entries: vec![EntryView {
                key: "entry-1".to_string(),
                title: "Entry One".to_string(),
                link: "https://example.com/entry-1".to_string(),
                content: "Example content".to_string(),
                published: None,
                is_read: false,
            }],
            seen_keys: vec!["entry-1".to_string()],
        },
    );

    let state = AppState { version: 1, feeds };
    store::save_state(&path, &state).expect("save state");
    let loaded = store::load_state(&path);

    assert_eq!(loaded.version, state.version);
    assert_eq!(loaded.feeds.len(), 1);
    let feed_state = loaded
        .feeds
        .get("https://example.com/feed")
        .expect("feed state present");
    assert_eq!(feed_state.title.as_deref(), Some("Example Feed"));
    assert_eq!(feed_state.entries.len(), 1);
    assert_eq!(feed_state.entries[0].key, "entry-1");

    let _ = fs::remove_file(path);
}

#[test]
fn load_state_returns_default_on_version_mismatch() {
    let path = temp_path("state_version", "json");
    let state = AppState {
        version: 2,
        feeds: HashMap::new(),
    };
    store::save_state(&path, &state).expect("save state");
    let loaded = store::load_state(&path);

    assert_eq!(loaded.version, AppState::default().version);
    assert!(loaded.feeds.is_empty());

    let _ = fs::remove_file(path);
}

#[test]
fn load_state_returns_default_on_invalid_json() {
    let path = temp_path("state_invalid_json", "json");
    fs::write(&path, "{ invalid ").expect("write invalid json");

    let loaded = store::load_state(&path);
    let expected = AppState::default();
    assert_eq!(loaded.version, expected.version);
    assert!(loaded.feeds.is_empty());

    let _ = fs::remove_file(path);
}
