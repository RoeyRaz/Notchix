# Nock Custom project conventions

## Scope

- The root `Cargo.toml` and `src/` are the active Windows source. `nock-custom/` is an older nested snapshot; do not edit or package it without an explicit migration plan.
- Notchix is the visible fork brand. Keep the existing MSIX identity, executable name, and `%APPDATA%\Nock` path until an explicit, tested migration is approved.
- Preserve Windows 11 behavior and existing Linux conditional compilation. Keep the MIT license and upstream attribution with distributions.
- Prefer native Rust, Win32, Direct2D and WinRT. Avoid webviews, persistent workers, and new polling when a Windows event is available. Tick animations only while visible.
- Keep `WM_APP` IDs unique. Keep external calls bounded and off the UI thread. Do not log credentials, private calendar URLs, notification text, or clipboard content.
- Treat process I/O as activity only, never proof of a download or a source of percentage or ETA. Only display measured progress from a source that supplies a real total, and label unknown ETA as unknown. Browser playback and calls must not appear as downloads.
- Keep media primary in the collapsed pill; transfer progress is a small secondary indicator. Camera and microphone states are independent small indicators.
- For sound changes, use an original asset, respect mute and notification preferences, and avoid blocking the UI thread.
- Live Glass may read only the island-sized area behind its own windows, must keep pixels in memory, and must never save or log captures. Keep capture and blur off the UI thread, throttle unchanged scenes, and fall back to the standard shell if capture fails. Glass uses display affinity to exclude Notchix from screen captures, so check screenshot and recording behavior explicitly.
- Notion quick capture sends only after a deliberate click and uses a protected token. Do not claim Hebrew voice support until microphone transcription has been tested with a local engine.

## Validation checklist

1. Run `cargo fmt --check`, `cargo build --locked`, and `cargo test --locked` on Windows MSVC. Run `cargo clippy --locked --all-targets` when available.
2. Build `cargo build --release --locked`; verify the executable starts on Windows 11 without another Nock instance running.
3. Exercise Spotify play/pause, skip, artwork, clock, volume, and the music-plus-transfer layout. Check browser video and voice calls cause no download claim.
4. Check qBittorrent progress and ETA against its own UI, plus API unavailable and authentication failure cases. Confirm no invented values.
5. Check notification consent in an installed MSIX, new versus historical toasts, privacy, quiet mode, and sound. Check mic and camera indicators with real capture apps.
6. Check calendar feeds and time zones, audio switching, shelf drag/drop, mirror lifecycle, calculator, typing game, top bar, and full-screen/DPI/multi-monitor behavior.
7. Measure idle CPU, private bytes, wakeups, animation frame pacing, and startup time on a Windows 11 machine. Record numbers and hardware before making performance claims.
   For Live Glass, compare moving and static content behind both collapsed and expanded shapes; check open/close for stale frames, multi-monitor and DPI transitions, games/fullscreen, and Glass off. Compare CPU and memory against the same build with `NOTCHIX_LIVE_GLASS=0`.
8. Check installer/MSIX install, update, uninstall, signatures, and attribution before release.

Do not mark a feature as working merely because it compiles or a preview renders.
