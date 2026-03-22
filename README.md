# RSS Reader (Rust)

Simple TUI RSS/Atom reader that shows feed entries in a terminal UI.

## Requirements
- Rust 1.88+

## Build
```bash
cargo build
```

## Run
```bash
cargo run
```

Default feeds are `https://hnrss.org/frontpage` and `https://tech.meituan.com/feed/` when no feed is provided.

```bash
cargo run -- "https://example.com/feed.xml"
```

## Common Options
```bash
cargo run -- --max-items 20 --seen-cap 5000
cargo run -- --timeout-secs 15 --user-agent "my-reader/1.0"
```

## Multi-feed Usage
Use repeated `--feed` flags or a feeds file with `--feeds`. The `--user-agent` option remains available.

```bash
cargo run -- --feed "https://example.com/feed.xml" --feed "https://example.net/rss" --user-agent "my-reader/1.0"
```

Feeds file format rules:
- One URL per line
- Blank lines are ignored
- Lines starting with `#` are comments

```bash
cargo run -- --feeds feeds.txt --user-agent "my-reader/1.0"
```

State is persisted at `~/.config/rss-reader/state.json`.

## Keybindings
- `j`/`k` or arrow keys to move
- `Tab` to switch panes
- `Enter` to open the selected item
- `r` to refresh
- `q` or `Esc` to quit

## Notes
- The reader keeps a rolling in-memory set of item IDs/links to avoid duplicates.
- If a feed lacks IDs and links, it falls back to title/summary-based keys.
- Interval and seen-cap values are clamped to at least 1 to avoid busy loops or empty caches.
