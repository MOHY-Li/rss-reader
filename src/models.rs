use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryView {
    pub key: String,
    pub title: String,
    pub link: String,
    pub is_read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedState {
    pub feed_url: String,
    pub title: Option<String>,
    pub entries: Vec<EntryView>,
    pub seen_keys: Vec<String>,
    pub read_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub version: u32,
    pub feeds: HashMap<String, FeedState>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: 1,
            feeds: HashMap::new(),
        }
    }
}
