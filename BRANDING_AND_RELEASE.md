# Notchix branding and publication notes

Notchix is an independently branded derivative based on [Nock by Akhilesh Varute](https://github.com/Akhilesh-Varute/Nock). Preserve the upstream MIT license and copyright notice in source and distributions. Keep the original author's credit visible in the README and included LICENSE; Notchix changes are credited to Notchix contributors.

## Identity and compatibility

- Visible product, Start menu, installer and package identity: **Notchix**.
- MSIX identity: `Notchix.Desktop`, publisher `CN=Notchix Developer`.
- Executable filename remains `nock.exe` for compatibility with existing scripts and file associations.
- User data remains in `%APPDATA%\Nock` so current settings survive the visual and installer rebrand.
- GitHub repository URL identifies the hosting account. Public contributor identity is governed by the repository's Git history and GitHub account settings; renaming source files does not rewrite past commits or remove that attribution.

## Release checks

1. Build and test with Windows MSVC; build the Inno Setup installer and MSIX locally.
2. Verify install, update, start with Windows, notification access and uninstall. A package identity change from the prior private build may require uninstalling that old package once; keep `%APPDATA%\Nock`.
3. Confirm the Notchix updater points to this repository and downloads the Notchix filenames.
4. Scan tracked files and generated release assets for tokens, private calendar links, machine-local paths, personal package identity and test credentials.
5. Check the included setup guide, MIT notice, bundled icon, mascot, fonts, sound and other assets for accurate attribution and redistribution terms.
6. Profile idle and active CPU/RAM on Windows 11 hardware before making resource claims.

The GitHub account name in repository URLs is necessarily visible while that account owns the repository. To change it, change the repository owner or transfer the repository; source cleanup alone cannot change that hosting identity.
