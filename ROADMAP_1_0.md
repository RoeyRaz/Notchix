# Notchix 1.0 release checklist

## Release blockers

1. **Windows 11 end-to-end run:** test installed MSIX notification permission, fresh versus historical notices, quiet mode, sound, camera/microphone indicators, and launch/startup behavior.
2. **Transfer accuracy:** compare qBittorrent percentage and ETA with its UI, test unavailable/bad API credentials and multiple torrents. Test Explorer copy/delete dialogs and Prism/Zen/Firefox transfer cases. Label only actual source-reported progress; keep media primary and avoid browser playback/call false positives.
3. **Core Windows integrations:** test Spotify playback, volume and OAuth queue; calendar feeds/time zones; audio switching; shelf/file drag-and-drop; mirror lifecycle; converter; CSV quick profile; calculator; typing game; full-screen/DPI/multi-monitor behavior.
4. **Glass and interaction:** test moving and static background content behind the idle and expanded island, open/close stale-frame clearing, glass-off rendering, screenshots/recording behavior and accessibility.
5. **Resource profile:** measure idle CPU, private bytes, wakeups, startup and animation smoothness on Windows 11 hardware. Publish the measurement context; make no unmeasured efficiency claims.
6. **Release packaging:** validate installer and MSIX install/update/uninstall, Start menu and startup behavior, signature prompts, package identity migration and preservation of `%APPDATA%\Nock`.
7. **Privacy and provenance:** scan source and release artifacts for credentials, private feeds, user-specific paths and personal package identity. Preserve the upstream Nock MIT notice and asset attribution.

## Deliberate release exclusions

- The experimental Notchix taskbar replacement is removed. Windows Explorer remains the taskbar.
- The unverified PresentMon/League FPS strip is removed. No game FPS values are shown.
- There is no Ollama runtime or local-AI feature in this release. The tray has no top-bar toggle; the existing top status strip remains part of the app.

## After 1.0

- Broader statistical views and charts for CSV profiles, only if requested and profiled.
- More activity integrations where a supported API provides genuine progress.
- Jarvis/AI connections only with explicit local/network boundaries, opt-in settings and tested actions.

Taskbar replacement is not planned for this release. Revisit only with a concrete design, full Explorer feature inventory, recovery path and measured resource/frame pacing plan.
