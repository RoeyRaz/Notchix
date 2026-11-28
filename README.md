<p align="center"><img src="assets/notchix_icon256.png" width="96" alt="Notchix icon"></p>

<h1 align="center">Notchix</h1>

<p align="center">A lightweight Windows desktop island, written in Rust.<br>Media, glanceable tools and glass UI without replacing Explorer.</p>

> Notchix is a derivative of [Nock by Akhilesh Varute](https://github.com/Akhilesh-Varute/Nock). It retains the upstream MIT license and copyright notice in [LICENSE](LICENSE), with credit to the original author.

<p align="center">
<img src="https://img.shields.io/badge/platform-Windows%2011-blue" alt="Windows 11">
<img src="https://img.shields.io/badge/license-MIT-green" alt="MIT">
</p>

Download the latest Windows installer from [GitHub Releases](../../releases/latest). The installer is per-user, adds a Start menu shortcut, and opens a setup guide. Portable builds are also published. See [Setup guide](docs/SETUP_GUIDE.html) and [release checklist](ROADMAP_1_0.md).

## Included

- Expandable top island with media controls, lyrics, calendar, activity, clipboard, shelf, tools, and system stats.
- Windows media session integration, including Spotify playback and the optional quick Spotify queue connection.
- Local CSV/TSV quick profile in Tools. Select a file to see row/column counts, missing cells, and numeric summaries. Files are processed locally with size and row limits.
- Drag-and-drop file conversion for supported audio, video, and image formats when FFmpeg is available.
- Notifications, agenda, qBittorrent progress, and optional integrations described in the setup guide.
- Optional live glass effect. Its appearance depends on Windows capture/composition behavior and may be unavailable over some protected or accelerated windows.

The Windows taskbar remains under Explorer's control. The experimental Notchix taskbar replacement and in-game FPS overlay have been removed. Older preview builds may have changed Explorer's auto-hide state; this build restores a saved state marker on startup.

## Build from source

On Windows 11, install Rust with the MSVC toolchain and Visual Studio Build Tools with the Desktop development with C++ workload. Then run:

```powershell
cargo build --release
cargo test --target x86_64-pc-windows-msvc
```

For the installer, install [Inno Setup 6](https://jrsoftware.org/isinfo.php), then run:

```powershell
ISCC.exe /DMyAppVersion=0.13.0 installer\notchix.iss
```

See [AGENTS.md](AGENTS.md) for project conventions and the validation checklist. See [CHANGELOG.md](CHANGELOG.md) for the project history.

## Privacy

The CSV profiler does not upload or retain the selected file. Other integrations may make network requests when configured, such as calendar feed sync, qBittorrent's local Web UI, or Spotify's API. Setup details and limitations are described in the [guide](docs/SETUP_GUIDE.html).

The repository retains its existing Linux preview builds; Windows 11 is the primary release target. Linux support receives less validation and has a smaller feature set.
