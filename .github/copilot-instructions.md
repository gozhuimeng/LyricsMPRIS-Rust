# Copilot instructions (LyricsMPRIS-Rust)

## Quick commands
- Build: `cargo build` (release: `cargo build --release`)
- Run (TUI): `cargo run --` (pipe: `cargo run -- --pipe`)
- Logs (to stderr): `RUST_LOG=debug cargo run --`
- Tests: `cargo test`

## Big picture (data flow)
- `src/pool.rs`: spawns the long-running event loop via `pool::listen(...)`.
- `src/mpris/events.rs`: watches D-Bus (MPRIS + playerctld) and emits `Event::Mpris(...)` into a channel.
- `src/event.rs`: central coordinator:
  - updates `StateBundle` (player + lyrics),
  - fetches lyrics (DB cache → providers),
  - sends immutable `Update` snapshots to the UI via `send_update(...)` with version/change suppression.
- `src/state.rs`: **mutable internal state** + **immutable snapshots** (`Update` holds `Arc<Vec<LyricLine>>`). UI should consume `Update`, not mutate shared state.
- `src/ui/modern.rs` + `src/ui/pipe.rs`: consume `Update` stream and use local position estimation for smooth progression.

## Repo-specific conventions & patterns
- Event-driven only: **do not introduce periodic timers / polling loops** for core logic. Prefer routing new information as `Event` → `process_event(...)` (see `src/event.rs`) driven by D-Bus signals.
- Existing exceptions: there are a few small `sleep(...)` calls in `src/mpris/events.rs` used only for safety (no-active-player backoff, disconnect detection) and should not become a general pattern.
- Change suppression: UI updates are throttled using a composite `(version, playing)` key (see `LAST_SENT_VERSION` in `src/event.rs`). If you add new state that should trigger redraws, ensure it bumps the state version via `StateBundle`.
- Lyrics fetch pipeline (important):
  1. If `--database PATH` is set, `src/main.rs` calls `lyrics::database::initialize(...)`.
  2. `src/event.rs` tries `lyrics::database::fetch_from_database(...)` first.
  3. On miss, providers are tried in configured order (`--providers ...` or `LYRIC_PROVIDERS`).
- Provider semantics: returning `Ok((Vec::new(), None))` means “no lyrics / provider unavailable” (e.g. Musixmatch without `MUSIXMATCH_USERTOKEN`). Network errors are treated as transient to allow fallback.
- Adding a lyrics provider:
  - Implement in `src/lyrics/providers/<name>.rs` returning `ProviderResult` (`(Vec<LyricLine>, Option<String>)`).
  - Wire it into `try_provider(...)` in `src/event.rs` (provider name strings are lowercase).
  - If it has a distinct format, extend `Provider` in `src/state.rs` and add DB format detection/storage in `src/event.rs` + `src/lyrics/database.rs`.
- Karaoke timing: only `Provider::MusixmatchRichsync` is treated as word/grapheme-timed. Scheduling lives in `src/ui/progression.rs`; rendering logic/caching is in `src/ui/modern_helpers.rs`.
- UI progression: use boundary-based scheduling (`compute_next_word_sleep_from_update` / `estimate_update_and_next_sleep`) rather than fixed-interval ticks.
- Output hygiene: logs are configured to go to **stderr** so stdout remains clean for `--pipe` (see `src/main.rs`).

## Integration points / external deps
- D-Bus via `zbus` (session bus). Player discovery depends on `playerctld` when available (see `src/mpris/connection.rs`).
- HTTP via a shared `reqwest::Client` (`src/lyrics/types.rs`). Musixmatch requires `MUSIXMATCH_USERTOKEN`.
- SQLite cache via `sqlx` with a global pool (`src/lyrics/database.rs` creates schema and uses WAL mode).
