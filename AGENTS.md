# Notchix project conventions

## Scope and attribution

- The root `Cargo.toml` and `src/` are the active source. `nock-custom/` is a historical nested snapshot; do not edit or package it without an explicit migration plan.
- Notchix is the user-facing product. Preserve Windows 11 behavior, Linux conditional compilation and existing settings under `%APPDATA%\Nock` unless a migration is specifically approved.
- Notchix is based on Nock by Akhilesh Varute. Keep the upstream MIT license/copyright in all source and distributions, and credit the original author in public materials.
- Do not place personal machine paths, personal package identities, tokens, private calendar URLs or user-specific credentials in tracked files. GitHub hosting-account names in repository links are controlled by repository ownership, not source rebranding.

## Implementation rules

- Prefer native Rust, Win32, Direct2D and WinRT. Avoid webviews and permanent workers when a user-triggered worker suffices. Animate only while visible or active.
- Keep `WM_APP` IDs unique. Run file/network parsing off the UI thread; bound memory, input size and execution scope. Do not log credentials, private calendar URLs, notification text, or clipboard content.
- Never infer a transfer percentage or ETA from process I/O or playback. Display measured values only when a source reports an actual total/progress; browser media and calls are not downloads.
- Media remains primary in the collapsed pill; real transfer activity is secondary. Camera and microphone indicators stay separate.
- Keep the Windows taskbar as the system taskbar. Do not reintroduce the Notchix taskbar replacement or in-game FPS overlay without a separately approved, profiled plan.
- Preserve the local settings directory and restore any saved Explorer taskbar state from older previews at startup.
- Keep Live Glass capture bounded to the island-sized area, in memory, off the UI thread; throttle unchanged scenes and fall back if capture fails. Check display-affinity behavior in screenshots and recordings.
- Notion only sends after an explicit user action and stores secrets with Windows protection. Do not claim untested voice or AI support.

## Validation checklist

1. Format Rust files touched by the change with `rustfmt --edition 2021 <files>`. Run `cargo fmt --all -- --check`; the current repository has pre-existing formatting drift across legacy files, so avoid reformatting unrelated modules just to silence that baseline. Run `cargo build --locked`, `cargo test --locked` and `cargo clippy --locked --all-targets` where the toolchain supports them.
2. Run Windows MSVC `cargo check --target x86_64-pc-windows-msvc` and `cargo build --release --locked`; launch the app on Windows 11 and verify a single instance.
3. Run installer build, silent install/uninstall smoke check, Start menu shortcut check and update-path check. For MSIX, validate package identity, certificate and notification consent. Preserve `%APPDATA%\Nock`.
4. Exercise Spotify controls and queue, notification privacy/quiet mode, mic/camera indicators, qBittorrent genuine progress and ETA, calendar/time zones, audio switching, converter and CSV quick profile.
5. Verify drag/drop, mirror start/stop, full-screen hide, DPI/multi-monitor behavior, glass appearance and stale-frame clearing on the actual Windows 11 host.
6. Scan tracked files and release artifacts for tokens, private calendar links, machine-local paths and personal identity. Retain original Nock license and credit.
7. Profile idle CPU/RAM, wakeups, animation frame pacing and startup time before publishing performance claims.

Do not call a feature functional merely because it compiles or its screen renders. Record live environment limitations.
