use clap::Parser;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "rss-reader",
    version,
    about = "Simple RSS/Atom reader with auto polling"
)]
pub struct Args {
    #[arg(value_name = "URL")]
    pub url: Option<String>,

    #[arg(long = "feed", action = clap::ArgAction::Append, value_name = "URL")]
    pub feeds: Vec<String>,

    #[arg(long = "feeds", value_name = "FILE")]
    pub feeds_file: Option<PathBuf>,

    #[arg(long, default_value_t = 20)]
    pub timeout_secs: u64,

    #[arg(long, default_value = "rss-reader/0.1")]
    pub user_agent: String,

    #[arg(long, default_value_t = 50)]
    pub max_items: usize,

    #[arg(long, default_value_t = 2000)]
    pub seen_cap: usize,
}

#[derive(Debug)]
pub enum FeedResolveError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidUrl {
        value: String,
        line: Option<usize>,
    },
    NoFeeds,
}

impl fmt::Display for FeedResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeedResolveError::Io { path, source } => {
                write!(
                    f,
                    "failed to read feeds file {}: {}",
                    path.display(),
                    source
                )
            }
            FeedResolveError::InvalidUrl { value, line } => match line {
                Some(line_number) if *line_number > 0 => {
                    write!(f, "invalid feed URL at line {}: {}", line_number, value)
                }
                _ => write!(f, "invalid feed URL: {}", value),
            },
            FeedResolveError::NoFeeds => write!(f, "no feeds provided"),
        }
    }
}

impl Error for FeedResolveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FeedResolveError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Args {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    pub fn resolve_feeds(&self) -> Result<Vec<String>, FeedResolveError> {
        let mut feeds = Vec::new();
        let mut seen = HashSet::new();

        for feed in &self.feeds {
            let validated = validate_feed_url(feed, None)?;
            if seen.insert(validated.clone()) {
                feeds.push(validated);
            }
        }

        if let Some(path) = &self.feeds_file {
            let file_feeds = parse_feeds_file(path)?;
            for feed in file_feeds {
                if seen.insert(feed.clone()) {
                    feeds.push(feed);
                }
            }
        }

        if feeds.is_empty() && self.feeds_file.is_none() {
            if let Some(url) = &self.url {
                let validated = validate_feed_url(url, None)?;
                if seen.insert(validated.clone()) {
                    feeds.push(validated);
                }
            } else {
                for default_feed in [
                    "https://hnrss.org/frontpage",
                    "https://tech.meituan.com/feed/",
                ] {
                    let validated = validate_feed_url(default_feed, None)?;
                    if seen.insert(validated.clone()) {
                        feeds.push(validated);
                    }
                }
            }
        }

        if feeds.is_empty() {
            return Err(FeedResolveError::NoFeeds);
        }

        Ok(feeds)
    }
}

pub(crate) fn parse_feeds_file(path: &PathBuf) -> Result<Vec<String>, FeedResolveError> {
    let contents = fs::read_to_string(path).map_err(|source| FeedResolveError::Io {
        path: path.clone(),
        source,
    })?;
    let mut feeds = Vec::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        feeds.push(validate_feed_url(line, Some(index + 1))?);
    }

    Ok(feeds)
}

pub(crate) fn validate_feed_url(
    value: &str,
    line: Option<usize>,
) -> Result<String, FeedResolveError> {
    url::Url::parse(value)
        .map(|_| value.to_string())
        .map_err(|_| FeedResolveError::InvalidUrl {
            value: value.to_string(),
            line,
        })
}
