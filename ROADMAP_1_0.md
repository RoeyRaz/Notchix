# Nock custom: 1.0 release checklist

## Audit snapshot — 11 November 2026

The root source tree is active; `nock-custom/` is an older nested copy. The
folder is a source snapshot without Git metadata, so provenance and diffs
cannot be verified locally. Windows MSVC compilation exposed a dock message
match bug, now corrected. The test suite exposed an outdated calendar
expectation after the lookahead changed to 14 days, now corrected. A successful
compile or unit test is not evidence that Windows integrations work at runtime.

### Priority 0 — release blockers

1. **Windows 11 integration pass:** build and launch the root executable, then
   install the MSIX and test notification consent, real new toasts versus
   history, quiet mode, and capture indicators. Portable EXE cannot validate
   the notification listener's package identity path.
2. **Transfer truthfulness:** compare qBittorrent percentage and ETA with its
   own UI, including no API key, bad key, offline app and multiple transfers.
   Browser transfer activity currently requires a growing partial file, but
   Steam/Epic/Battle.net still use process I/O as a weak activity signal. Do
   not label that signal a verified download or derive progress from it.
3. **Release packaging and rights:** verify MSIX certificate trust, installer
   install/update/uninstall, upstream MIT notices and every bundled asset's
   redistribution rights. Align crate, manifest and installer versions.
4. **Regression matrix:** exercise Spotify controls and app volume, audio
   switching, multi-calendar time zones, shelf drag/drop, mirror camera
   lifecycle, calculator, typing game, top bar, full-screen and multi-monitor
   DPI behavior. Record pass/fail per scenario.
   The experimental League strip additionally needs Intel PresentMon installed,
   a real match, borderless/exclusive checks, and frame-time/CPU comparison
   with the strip on and off before calling it ready.

### Priority 1 — quality and performance

5. Profile idle CPU, private bytes, wakeups, animation frame pacing and startup
   on Windows 11 hardware. The mascot has a 55 ms timer when visible, transfer
   sampling runs every 2 seconds, and the notification fallback timer runs
   every 450 ms; measure their cost before changing the scheduling design.
6. Audit calendar parsing for DST, all-day events and recurrence exceptions;
   the current parser documents time-zone limitations. Improve keyboard and
   screen-reader access, text clipping and Hebrew rendering with real users.
7. Validate the new Nock smart-alert sound at low and high system volume and
   with notification mute. Windows toasts may still play their own system sound;
   changing that needs an explicit user-facing choice to avoid double audio.

### Priority 2 — useful additions after the gate

8. Add an opt-in Jarvis bridge over a local named pipe or loopback socket with
   a narrow, versioned command set: open panel, show a short status, set focus
   timer and query current media. Require explicit confirmation for power,
   file and message actions; never expose private notifications by default.
9. Add richer download adapters only where a real API provides transferred
   bytes and total size. Add a compact audio-device chooser, calendar source
   status, and keyboard shortcuts before considering a taskbar replacement.
   For exports in other applications, first capture a real job in that app and
   verify its own progress API or accessible progress control. Keep completion
   notifications separate from live progress, and never infer a percentage
   from process activity or a growing output file.
10. Keep the top bar optional. Profile a small companion dock separately and
    leave Explorer as the taskbar until a measured use case justifies more.

**Architecture decision needed:** any persistent Jarvis service, broad shell
integration, or taskbar replacement needs review before implementation.

The goal is a fast, dependable Windows Dynamic Island, not a collection of
unrelated widgets. Keep the compact state calm and show live information only
when it matters.

## Must be done before 1.0

1. **Build and reliability:** automated Windows x64 builds and smoke tests,
   clean install/update/uninstall, and proper MSIX certificate handling.
2. **Actual live activities:** qBittorrent connector validation, reliable
   file-operation visibility where Windows allows it, and graceful fallback
   when a program has no progress API. Never invent a completion percentage.
3. **Notifications:** validate permission flow, per-app icons, dismiss/clear,
   privacy and behavior under Windows Do Not Disturb.
4. **Multi-monitor and fullscreen:** verify scaling, hover/click focus,
   maximized windows, games, display sleep and DPI changes.
5. **Resource usage:** profile idle CPU/RAM, foreground animations, webcam
   start/stop, process polling, and startup latency on laptop and desktop.
6. **Core usability:** keyboard navigation, inaccessible-calendar onboarding,
   media controls, bilingual text rendering and compact layouts.
7. **Safety:** clear clipboard/notification retention rules, secure API key
   storage (Windows Credential Manager), and a small set of diagnostics that
   never captures private notification text by default.

## Nice to have after 1.0

### Proper taskbar replacement

The requested target is a genuine taskbar mode, not the optional floating dock.
Use the screenshot references and Fenn/NotchPixel as product inspiration, with
an original Notchix layout: a low glass rail that grows only as wide as its contents; a compact Start and
search area; grouped pinned and running windows in the center; and a system
area with tray, audio/network/power, clock and notification access. Live window
previews, right-click window menus, badges and multiple monitors belong in the
interaction design. The neutral blur must reflect the wallpaper and windows;
purple stays an accent and album colours remain mainly in media text.

The existing dock code is an interaction experiment, not this taskbar. A
taskbar mode must first cover Explorer's essential jobs: launch and switch
windows, Start/search, system tray and overflow, notification and quick
settings access, work-area reservation, display/DPI changes, fullscreen
handoff and crash recovery. Keep an immediate way to restore Explorer's
taskbar. Measure idle CPU, memory, preview redraws and game frame pacing before
making it the default. The current opt-in preview temporarily enables Explorer
auto-hide, leaving its system tray and other controls accessible at the screen
edge. Keep that recovery path until the missing taskbar functions are built and
tested. Implement the taskbar as a separate mode and retire the
floating dock after feature parity; do not relabel the dock as a taskbar.

Current prototype: an opt-in, partial-width bottom rail now shares the island's
background-capture blur and adapts to pinned-app count. Start, Search, pinned
apps, running marks, audio access and clock/Agenda are wired. Version 0.13.8.37
adds a bounded set of unpinned visible running apps, with click-to-switch code;
that interaction still needs a live check. Version 0.13.8.31
was visually confirmed at the bottom with working blur. Explorer auto-hides while
the preview is active and returns when it is disabled or the app relaunches.
Complete live window groups, tray, previews and work-area reservation remain before
replacement mode; recovery has only been checked for the preview path.

### Game strip study

For League of Legends, keep the strip off by default until a real game session
can verify visibility and input behavior. Only show it in borderless play when
it does not cover the game HUD; leave exclusive fullscreen alone. Reuse the
existing measured CPU, RAM and clock sources. FPS and GPU require a supported
external telemetry source such as PresentMon; show an unavailable mark instead
of fabricated values. Do not read League process memory or inject into the
game. Compare FPS/GPU against an independent monitor, then profile idle and
in-game overhead before enabling the strip.

- Better download integrations for browsers and launchers where supported.
- More Miso expressions and opt-in contextual responses.
- WPM statistics, optional Hebrew prompts and adjustable sprint length.
- Per-user themes and deeper customization, while preserving minimalist defaults.

## Public release gate

Before GitHub publication, identify the upstream Nock license and comply
with its copyright and attribution requirements. Review bundled icon, font,
mascot and other third-party asset licenses. Rebrand only after the license
audit. New branding does not remove upstream license obligations.
