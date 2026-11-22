# Notchix branding and publication notes

Notchix is Roey's customized derivative of [Nock by Akhilesh Varute](https://github.com/Akhilesh-Varute/Nock). Describe the project as a fork or derivative with original Notchix changes, not as an independently written codebase. Retain the upstream `LICENSE` copyright and MIT permission notice in source and distributions. The [MIT license](https://opensource.org/license/mit) permits modification and distribution subject to retaining those notices.

## Current visual direction

- Distinct open-orbit icon with a dark center and small status light, rather than the upstream notch/equalizer tile. The new icon was developed for this fork; `assets/notchix_icon256.png`, `assets/notchix_icon.png`, and `assets/notchix.ico` are the shipping files.
- Neutral graphite glass with a restrained tint sampled from current media artwork. No new polling, window, image decode, or animation timer was added for the tint.
- Visible product name is Notchix. The MSIX package identity `Roey.NockCustom`, executable `nock.exe`, `%APPDATA%\Nock` data folder, and developer certificate subject remain temporarily unchanged to preserve installed updates and settings. These internal identifiers do not define the public brand.

## Before a public GitHub release

1. Finish Windows 11 runtime and resource profiling in `ROADMAP_1_0.md`. Capture current Notchix screenshots and replace the upstream demo, release links, badges, and feature claims in `README.md`.
2. Audit the provenance and redistribution rights of every retained image, font, sound, mascot and demo asset; replace anything uncertain. Keep upstream authorship clear in the README, `LICENSE`, and package notices.
3. Point the updater at a Notchix release repository and add a tested update path. The current menu is explicitly labeled **Check upstream Nock releases** because its existing code still downloads upstream Nock.
4. Decide whether to migrate the MSIX identity, executable name, data directory and legacy `Nock.lnk` shortcut. That is an installation and settings migration, not a visual-only change; test update and rollback before doing it.
5. Create a Git repository from a traceable source tree. This supplied source folder has no `.git` history, so record the upstream commit or release used as the base before publishing.

Suggested repository summary: **Notchix is a lightweight Windows desktop island built on Nock by Akhilesh Varute. This fork adds its own branding, glass visuals and productivity integrations while retaining the upstream MIT license and credit.**
