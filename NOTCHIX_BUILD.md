# Notchix source build and local install

Notchix is based on [Nock by Akhilesh Varute](https://github.com/Akhilesh-Varute/Nock) and distributed under the upstream MIT license. See `LICENSE` and `BRANDING_AND_RELEASE.md` for attribution and release notes.

## Build the Windows app

1. Install the stable Rust MSVC toolchain and Visual Studio Build Tools with the Desktop development with C++ workload.
2. Open PowerShell in the repository and run `cargo build --release --locked`.
3. Launch `target\release\nock.exe` for a portable source build.

## Build the per-user installer

1. Install Inno Setup 6.
2. Run `ISCC.exe /DMyAppVersion=0.13.16 installer\notchix.iss` after building.
3. The installer is created in `installer\Output` and installs to `%LOCALAPPDATA%\Programs\Notchix`.

## Build the MSIX for Windows notification integration

Run `packaging\windows\Build-NotchixMsix.ps1` to build a local developer-signed package. Review its certificate thumbprint before trusting it. `packaging\windows\Install-Notchix.ps1` installs only after confirming that certificate is already trusted; it does not change Windows notification settings or import a certificate automatically.

The package identity is now Notchix-specific. Upgrading from an earlier private package identity may require removing that old MSIX once. Preserve `%APPDATA%\Nock` to keep settings, calendar feeds, tokens and local data.

## Setup guide and optional services

The installer opens `docs\SETUP_GUIDE.html`, which links to Spotify queue setup, qBittorrent setup, calendar privacy notes and Notion setup. The quick CSV profile is local and bounded to 32 MB, 250,000 rows and 128 columns.

Notchix does not replace the Windows taskbar and does not show an FPS overlay. On first launch after upgrading from the preview, it restores the saved Windows taskbar auto-hide state if one exists.
