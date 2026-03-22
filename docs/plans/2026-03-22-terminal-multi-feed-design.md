# Terminal Multi-Feed RSS Reader Design

## Summary
Add multi-feed support with two modes: default CLI output and optional TUI mode (`--tui`). Keep the core fetch/parse/dedupe pipeline shared across both modes.

## Goals
- Support multiple feed URLs via CLI or config file.
- Provide a TUI for browsing feeds and entries, while keeping the current CLI output mode.
- Preserve the existing fetch/parse logic and extend it to handle many sources concurrently.

## Non-Goals
- Full-text article fetching or HTML rendering.
- Persistent storage or sync across devices (future work).
- Complex filtering/search beyond basic source and unread filtering.

## Proposed CLI Interface
- `--feed <url>` (repeatable)
- `--feeds <path>` (file with one URL per line)
- `--tui` (launch interactive terminal UI)
- Existing flags remain (`--interval-secs`, `--timeout-secs`, `--user-agent`, `--max-items`, `--seen-cap`, `--once`).

## Architecture

### Modules
- `config`: parse CLI args, read feeds file, merge into a feed list.
- `fetcher`: async fetching per feed with shared HTTP client.
- `parser`: feed parsing via `feedparser-rs`.
- `store`: per-feed dedupe cache and unread tracking.
- `cli`: prints new items as they arrive.
- `tui`: interactive terminal interface (feed list + entry list + status).

### Data Model
- `FeedConfig { url, title_override? }`
- `FeedState { last_error?, seen_set, seen_order, entries }`
- `EntryView { title, link, published, feed_title, unread }`

## Data Flow
1. Load feed list from `--feed`/`--feeds`.
2. Start a scheduler that periodically triggers fetch for each feed.
3. Fetch -> parse -> dedupe -> update `FeedState`.
4. Output:
   - CLI: print new entries per feed.
   - TUI: refresh UI state (feed list + entries).

## Error Handling
- Fetch/parsing failures are isolated per feed.
- TUI shows recent errors in a status area.
- CLI logs errors to stderr, continues polling.

## TUI Behavior (Minimal)
- Feed list on left or top, entries list on right or bottom.
- Keyboard: `j/k` or arrows to navigate, `Enter` to open link (optional), `r` to refresh.
- Unread markers per feed and per entry.

## Testing
- Unit: key generation + dedupe behavior.
- Integration: parse local feed fixtures.
- Manual: CLI output with multiple feeds, TUI navigation and refresh.

## Milestones
1. Add multi-feed config and scheduler.
2. Extend store to per-feed dedupe and unread.
3. CLI multi-feed output.
4. TUI mode.
