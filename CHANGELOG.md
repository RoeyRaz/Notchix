# Changelog

## Unreleased (v1.0 preparation)

- Finalization pass: removed the experimental Notchix taskbar replacement and game FPS overlay, added one-time Explorer taskbar-state recovery, removed the dead Start-menu action, and restored the update check against Notchix releases.
- Added local CSV/TSV quick profiling with a 32 MB, 250,000-row, 128-column cap; numeric summaries and missing-cell counts; CSV quoting support; and focused tests. Parsing runs away from UI thread. No upload or data persistence.
- Rebranded MSIX identity and installer metadata to Notchix, kept `%APPDATA%\Nock` for settings continuity, and redirected updates to the Notchix repository. Rebuilt setup workflow and guide; first install from the old private MSIX identity may require a one-time uninstall.
- Removed user-specific package and filesystem names from current release metadata, renamed the launcher/add-on/build scripts, and updated install/release file names. Kept the upstream Nock license and attribution.
- Replaced the inherited Nock landing page with a Notchix-branded page, removed unverified performance claims and links to upstream download endpoints, and retained a clear Nock credit.

- Added an in-notch drag-and-drop converter panel with native format chips, a custom progress animation, and output beside the original file. FFmpeg and FFprobe are needed for conversion; measured progress appears only when duration is available. Conversion currently covers supported audio, video, and image formats, not arbitrary files. Corrected Shelf handling for multiple drops and made “Add to Shelf” use the converted output after a successful conversion. No Windows file picker is used in the drop flow. Windows target check, release build, and existing tests pass (54 passed, 8 ignored). A local FFmpeg image-to-JPEG smoke check preserved its source; the live drag/drop panel still needs a visual check.

- Paused further taskbar work after user feedback that the current prototype does not meet the intended design. Keep it optional and reversible while a new direction is considered.

- Continued the optional taskbar prototype: imported Windows pins on a best-effort basis, grouped visible app windows with counts and click-to-cycle behavior, added a hover title card, widened the Spotify artwork/player area, added an in-taskbar master-volume slider, and routed network/power and notifications to Windows' own panels. Fixed hover-card collapse while crossing into it and kept the reversible Windows taskbar handoff check active even while music is stopped. This is not a full Explorer replacement; live window previews, selectable window menus, custom network/power tray popovers, and Windows 11 runtime verification remain release gates.

- Spotify volume now keeps a quiet placeholder in Home when Windows has not created Spotify's app audio session; the Media player's unavailable state no longer collides with its source label. The control stays non-interactive until Windows exposes a real Spotify volume value.

- Mirror capture now requests the webcam package capability, redraws the panel when camera frames arrive, and only refreshes when frame/status sequence changes. The taskbar has a new partial-width, connected-glass layout with pinned/running apps, real media controls and progress, quick system shortcuts, and a compact clock. Windows MSVC check and optimized build pass; this environment has no camera device, so live capture and the taskbar's on-device blur still need Windows hardware verification. App grouping, window previews, and the full tray remain unfinished release work.

- Version 0.13.8.40 fixes Explorer file-operation detection for newer Windows 11 dialogs. The previous scanner rejected every top-level Explorer window whose class was not `#32770`; the screenshot's delete dialog can use another shell class even though its title contains the genuine `10% complete` value. Detection now checks the Explorer process, native progress controls, and the shell title without depending on that class. The parser test passes and the signed update was installed. A live sustained delete still needs confirmation.

- Version 0.13.8.39 adds the next optional taskbar step: up to four visible, unpinned running applications can appear beside pinned apps, with their native icons and click-to-switch behavior. The rail stays bounded at 680 logical pixels and remains disabled by default. Spotify queue search was investigated but not shipped because the current integration has no OAuth/API client; the existing Notion action remains until that flow is implemented safely. League PresentMon overlay remains unverified and should not be called functional. 54 tests passed, 8 ignored; the signed update was installed.

- Version 0.13.8.38 replaces the workday outline fallback with genuine media-session song progress when no timed event is active; the event outline retains priority. The existing saved toggle now reads "Song progress outline" and advances once per second only while a song with known duration is playing. Agenda refreshes in the background every five minutes and when opened after at least a minute; completion posts a UI message for immediate redraw instead of page polling. Installed diagnostics showed all five configured feeds downloaded successfully, but visual progress and end-to-end freshness still need observation. 54 tests passed, 8 ignored.

- Version 0.13.8.37 replaces the app and MSIX icons with a new Miso mascot asset, including small icon sizes. The optional adaptive taskbar preview now adds a bounded set of visible, unpinned running apps beside the existing pinned apps and switches to their windows when clicked. It does not replace the Windows tray or add hover previews yet. The signed update was installed and 52 tests passed, 8 ignored; running-app interaction remains to be checked live because the preview is currently off.
- Version 0.13.8.36 strengthened live blur and reduced transition latency by keeping the last blurred frame during resizing, refreshing resized captures sooner, and using a faster critically damped spring. Replaced the blur's per-pixel neighborhood scan with a sliding sum; a dedicated edge-average test confirms matching output. The signed update was installed before 0.13.8.37. A five-second sample of the intermediate 0.13.8.35 build used 1.28 CPU seconds with the current desktop state, which prompted the optimization; no comparable post-optimization performance claim is made.

- Version 0.13.8.34 tunes the idle acrylic: the live blurred backdrop is more visible, the dark wash is slightly stronger for legibility, and a restrained accent reflection sits along the lower center. The mascot now uses a neutral charcoal body and the same adaptive accent as the notch instead of teal and pink. The signed update was installed; 50 tests passed, 8 ignored. Visual quality still needs confirmation on the user's desktop because the window is excluded from screen capture.

- Corrected the 0.13.8.32 progress outline in 0.13.8.33: its path now follows the visible left side, rounded bottom, and right side of the top-attached island, with no line across the hidden top edge. Reworked the compact shell to clip its upper corners beyond the screen edge, reduced the dark fill over live blur, increased secondary text contrast, and delayed expanded controls until the shell is wide enough. Added a geometry test for three pill widths; 50 tests passed, 8 ignored. The signed MSIX was installed. A native visual inspection was unavailable in this session, so the appearance still needs confirmation.

- In 0.13.8.32, replaced the faint idle-only workday bar with a brighter outline that can remain visible with music, and added an event outline based on actual iCalendar `DTSTART`/`DTEND` times. Recurring occurrences retain the original duration; events without a valid end do not show progress. The collapsed shell now joins the top edge, has a lighter neutral glass fill, and a subtle top highlight. Windows tests passed (49 passed, 8 ignored); the signed MSIX was installed, but the visual appearance and live calendar behavior still need user confirmation.

- Started an opt-in adaptive taskbar preview: one partial-width glass surface sized to pinned apps, with native Windows Start and Search, running marks, audio access, clock/Agenda, and throttled background capture for blur. Rounded the compact island silhouette toward the references. Version 0.13.8.31 anchors the preview to the bottom, keeps it visible with ordinary maximized windows, and auto-hides Explorer's taskbar while the preview is on. A restore marker returns Explorer's prior taskbar state on normal exit or next launch after an unexpected exit. The installed geometry check measured 622×54 px at y=1023 on a 1920×1080 display, with capture exclusion active and Windows auto-hide state 1. The user confirmed its bottom placement and blur. A disable/relaunch/enable check confirmed Explorer's taskbar returned to its normal state and the preview re-enabled. Other click targets remain to be tested in use.

- Moved live-glass capture exclusion until after the island is shown, and made a temporary display-affinity failure retryable. In the installed check, blur appeared over Zen but not over ChatGPT while its computer-control overlay covered the desktop. A read-only Windows affinity check found that overlay set to `WDA_EXCLUDEFROMCAPTURE` (17), while the ordinary ChatGPT windows reported 0. The clean neutral surface remains the fallback where Windows withholds source pixels.
- Removed the unused floating-dock tray option in 0.13.8.29. The separate dock code remains disabled while the requested full taskbar mode is designed.
- Replaced the earlier dock-to-taskbar wording in the release plan with the requested full taskbar target: Start/search, running-window groups, system tray, quick settings, clock, work-area handling, and recovery. No taskbar replacement is installed yet.

- Removed the on-demand Ollama bridge and repurposed its Home entry as a quick Notion task capture. Typing prepares a task locally; the explicit Save action still controls sending. Simplified the tray by hiding Top bar, bar stats, reserve-space, Start-menu shortcut, and upstream release-check controls. Existing saved top-bar and layout settings remain intact.

- Repaired Start with Windows so an existing opt-in launches the installed Notchix package through its stable app identity, rather than an outdated portable executable or a versioned WindowsApps path. The installer now carries this repair forward on updates.
- Made browser activity require a growing partial file in Downloads and a running browser, with a lower threshold for slow transfers. The label stays generic when Zen and Firefox cannot be distinguished; no browser percentage is invented. A controlled Zen download created a growing `.part` file, while Firefox is not installed on the test PC.
- Fixed the collapsed island sizing and animation when a browser, qBittorrent or Explorer transfer begins or ends without music. Previously the transfer renderer could be given the idle pill width, making its label appear missing or clipped.
- Added a Windows 11 Explorer copy-dialog title fallback for Shell-reported percentages when the native progress control is absent. The parser passes tests; the complete installed UI path remains to be checked during a sustained copy.
- Added event-driven running marks to the optional dock for pinned apps with a matching visible window; clicking a running pin restores that window. The Windows taskbar remains available.
- Simplified Media into one glass surface with a smaller cover, wider title area and a decorative wave-shaped seek line; its colour split still uses the real playback position, and seeking and lyrics remain available. The visual change still needs an installed-screen check.
- Added an experimental, opt-in League game strip backed by Intel PresentMon API2. It recognizes the actual foreground match executable and a full-screen-sized window, stays click-through, and remains hidden until PresentMon supplies measured FPS. CPU/RAM come from Windows; GPU is shown only when the service exposes an adapter value. PresentMon is not installed on the test PC, so real telemetry, in-game placement and impact remain unverified; the setting defaults off.
- Added distinct Activity and island cards for explicit WhatsApp/Discord call and completed-download notifications, plus explicit export-complete toasts from other apps. Browser WhatsApp is branded only when its Windows notification identifies `web.whatsapp.com`; ordinary chat messages cannot create a call/download badge. Call previews last longer, and private previews now hide the title as well as the body. Classifier tests and a synthetic Activity render passed; real call and download toasts still need user testing.
- Added a secondary Explorer file-operation progress indicator for Windows' own copy dialog. It reads the dialog's native progress range on a short worker using the existing transfer timer, never estimates a percentage from process I/O, and does not read file names or destinations. A live Explorer-to-drive copy confirmed the detector can read the Windows value; the installed UI still needs a visual check.
- Fixed qBittorrent progress not appearing when its local Web API returns a complete response but keeps the HTTP connection open; the connector now reads the declared response length instead of waiting for disconnect.
- Added live background blur and a small edge displacement to the Windows island using screen pixels captured behind it. Capture and blur run on a worker at half resolution, slow down when the background is unchanged, and fall back to the previous appearance if capture fails. The user's live preview showed blur without lag and confirmed the old frame clears on close. Release idle samples ranged from 3.6% to 5.9% of one CPU core with about 92–98 MB working set; these numbers are from this PC and are not a general performance guarantee.
- Reverted the earlier Desktop Acrylic underlay after live feedback showed a flat gray surface and lag; it is not part of the new capture-based path.
- Replaced the bright white outlines around the island and shared cards with subdued dark edges.
- Kept the glass shell and cards neutral across album covers; artwork colour now stays on small text, controls and music indicators. Lowered the shell and card opacity so the captured, blurred background remains visible.
- Added an on-demand local Ollama mini chat, reachable from Home, with Hebrew text display and a default matching the installed qwen3.5:2b model. Added an explicit Notion task action and DPAPI-protected setup script; Notion remains unconfigured until the user supplies an integration token and data source ID. Hebrew microphone transcription is not yet included.
- Unified the primary navigation, Home, Media, Tools and Activity around a layered translucent glass surface with a light-catching rim. Raised the Activity render ceiling to fit its taller layout; previewed it with synthetic notifications. This is a composited glass treatment, not backdrop blur/refraction.
- Gave Activity its own taller expanded island, three spacious layered-glass notification cards, clearer separation between app/title/message, and a small Windows connection status line.
- Added an in-panel Glass toggle and icons to utility navigation and Activity actions. Increased expanded height so Activity controls fit, restored roomier notice rows, and added an always-visible current lyric line below the Media player. The existing volume level island remains available.
- Redesigned Tools into a capture/audio column and compact quick-launch grid. Rebuilt Activity as a tighter inbox with two rows of wider controls so Quiet, Private, and Clear all labels fit.
- Reworked expanded navigation as separate glass destinations with a slim utility strip. Controls is now an asymmetric workbench with the same nine actions; Agenda uses a day header and timeline-style event rows.
- The collapsed music island now sizes to the visible title and artist instead of always reserving the maximum media width. The optional dock is drawn as three separate glass islands for Notchix, pinned apps and clock/Agenda, reusing its existing window and clock timer.
- Refresh the optional floating dock clock on the existing minute event, so it does not stay at the time of its last interaction. No new timer was added.
- Replaced the cramped ten-tab strip with a larger five-destination primary row and a compact five-item utility row. Increased the expanded panel height to preserve content space. Rebuilt Media with separate artwork and information surfaces, a clearer title-to-progress reading path, and a dedicated transport row.
- Living Glass now uses a warm lavender fallback from 07:00 to 17:59 and a cooler violet from 18:00 to 06:59, while actual album art continues to set the media accent. The navigation is reordered to Home, Media, Activity, Agenda, Shelf, Clips, Apps, Tools, Stats, Power; the Media card has a subtler tinted surface and play control.
- Miso now has a rounded shell and ears, and appears in Home's empty artwork area plus Activity and Shelf empty states. The optional smart-alert tone is louder while retaining its soft fade.
- Introduced the Notchix visual brand: original open-orbit icon in the EXE, tray and MSIX; album-art colour now lightly tints the glass shell and Home media card. Kept the existing MSIX identity and settings path for update compatibility.
- Updated visible Start, tray and package names. Labeled the existing updater as an upstream Nock release check. Added a fork attribution and publication checklist.
- Fixed the dock's mouse-leave message match so click and mouse-activation handlers remain reachable.
- Replaced Nock's optional smart-alert Windows beep with a short original two-note sound. It remains off unless `smart_sound=1`; Windows' own notification sounds are separate.
- Updated the calendar recurrence test for the existing 14-day lookahead.
- Replaced two approximate angle constants that stopped the Clippy check.
- Added project conventions and a Windows 11 validation checklist in `AGENTS.md`.

These changes require runtime audio and UI checks on a Windows 11 desktop before release.

## 0.13.0

- Linux preview (X11 / XWayland): the pill, the nine tabs (Media through MPRIS, Controls, Today, Stats, Clipboard, Shelf, Apps, Games, Power) and the top bar with Wi-Fi, Bluetooth, sound, brightness, battery and calendar dropdowns. Packaged as a `.deb` and a user-level tarball; CI builds, tests and smoke-tests both. Not yet on Linux: game sounds, smart pill alerts, reminders, tray menu, buddy and scenes.

## 0.12.10

- Idle notch extras, all optional and event driven (no new polling or always-on timers): an idle glance next to the clock (tasks left, focus today, CPU and RAM, or the next meeting), a focus progress line along the bottom of the notch while a focus session runs (on by default), a workday progress line, and scroll-up cycling through time, date, tasks left and focus today. New tray items: *Idle glance*, *Show focus progress on the notch*, *Workday progress line on the notch*, *Scroll up over the notch cycles time, date, tasks*. New `config.ini` keys: `idle_glance`, `idle_focus_bar`, `idle_meeting_hours`, `day_line`, `day_start`, `day_end`, `idle_cycle`.
- Optional state dot on the idle and timer notch (steady dot, ring for mic, amber square for low battery, arc for a focus session) and optional buddy peeks over the bottom edge of the notch (robot and kitty, one long one-shot timer). New tray items *State dot on the notch* and *Buddy peeks over the notch now and then*; new `config.ini` keys `state_dot` and `peek`, both off by default.
- The `config.ini` file written by Nock now describes the current buddies.

## 0.12.9

- New Robot buddy, now the default: a bone-white robot with a blue status light, big eyes that follow the pointer, and three equaliser bars on its chest that dance with the music. It replaces the old robot.
- Removed the Blob, Bunny and Hero buddies. Existing settings that used them now show the Robot. Kitty and the three pixel characters stay.
- Removed the press-conference scene and its photo cut-out (`assets/face.png`). The other scenes are unchanged, and the tray menu keeps the same order.

## 0.12.1

- Auto-hide: new tray option that keeps the notch and bar out of sight until the pointer touches the top-center edge of the screen. Saved as `auto_hide=` in `config.ini`, off by default.

## 0.11.0

- Multi-display: the tray menu has a new *Show on display* submenu (when more than one display is connected) to put the notch, top bar, buddy and scenes on any display. Saved as `monitor=` in `config.ini`; falls back to the primary display if the chosen one is unplugged.

## 0.10.2

- Check for updates: an installed copy is now recognised by the uninstaller next to it (and a case-insensitive path match), so it offers to update itself instead of just opening the download page. The portable exe can update itself too: *Yes* downloads the new `Nock-portable-<version>.exe`, replaces itself and restarts; *No* opens the download page.

## 0.10.1

- The top-bar calendar can now move between months: previous/next arrows and a *Today* button to jump back.
- Clipboard tab: a scroll bar shows how long the list is and where you are in it.

## 0.10.0

- Important smart hints are harder to miss: meetings (blue), low battery (amber) and the break nudge (teal) glow in a strong colour and stay up for a minute or more. A meeting hint stays until the meeting starts or you click it, with a live countdown, and shows again for its last minute. New optional chime (tray → *Chime for meeting, battery and break hints*, `smart_sound=1`, off by default).

- Smart notch hints (tray → *Smart hints*, `smart` in config.ini, on by default): the pill shows an amber "Battery low" with the time left when unplugged at 15%, suggests a break after 90 minutes of steady keyboard/mouse use, shows "Mic on" with the time while an app is using the microphone (Windows notifies Nock when an app starts or stops recording, so it shows up at once), and says "Link copied", "Path copied", "Colour copied" (with a swatch) or "Code copied" for what you copy. Click the pill during a link or path hint to open it. While the mic is on, click the mic icon on the pill to mute or unmute the default microphone (it turns red). All local, using the existing once-a-minute tick and foreground-change event.

- Meeting hints: set `calendar_url` in config.ini to a read-only ICS link (Google: Settings → your calendar → *Secret address in iCal format*; Outlook: a published calendar). Five minutes before a meeting the pill shows its name and countdown, and clicking it opens the Zoom / Meet / Teams link found in the event. The link is fetched at start and about once an hour, only when you set it. Repeating DAILY and WEEKLY events are supported (not COUNT or monthly rules).
## 0.9.6

- Releases now publish the installer and a clearly named `Nock-portable-<version>.exe` instead of a bare `nock.exe`, with a note on the release page saying which to pick. Descriptions and file details now say "notch and top bar for Windows".

## 0.9.5

- Fixed the notch hiding when a Windows overlay (Start, Search, Task View and similar flyouts, which can cover the whole screen) was in front. These are no longer treated as full-screen apps. `visibility.log` showed this on a real PC: a `Windows.UI.Core.CoreWindow` the size of the monitor.

- Tray: *Close the panel when the pointer leaves* (`auto_close` in config.ini). Turn it off and the open panel stays until you close it. When another window takes focus and closes the panel, `visibility.log` now says which window did it.

## 0.9.4

- The joke scenes now have characters that fit them (developer, manager, coworkers, a bug, cricketers, a fan) instead of reusing the fantasy cast.
- The default buddy for new installs is now the Robot (existing settings are kept).

## 0.9.3

- Four new everyday-joke scenes with the original cast: Friday deploy, You're on mute, Low battery and Cricket six (tray menu → Scenes, and they join the random pick).

## 0.9.2

- Fixed the notch and bar staying hidden on some PCs even with nothing full screen. A maximised window on a taskbar set to auto-hide matches the monitor exactly and was mistaken for full screen; now a window must also have no title bar, and the Windows "busy" hint is only trusted for such a window.
- Tray: new *Hide in full-screen apps* switch (`fs_hide` in config.ini) to turn the auto-hide off entirely, and `visibility.log` records why Nock hid.
- Nock's windows no longer have titles, so window pickers in recorders such as OBS should stop listing them as separate windows.

## 0.9.1

- Tray menu → Check for updates: finds the latest GitHub release and, for the installer version, downloads, installs and restarts Nock for you (the plain exe opens the download page). Only runs when you click it.

## 0.9.0

- A softer, more colourful panel: Stats are inset tiles with a colour per metric, Today, Clipboard (colour by kind of item), Controls and Power (colour by severity) all picked up colour, the Media panel is tinted from the album art, and corners are rounder.
- The idle pill is wider and shows the date next to a bold time.
- Games: Thor in Viking Run and a new caped flyer in Super Flight (formerly Thor Flight) are drawn like the scene characters, with outlines, capes and proper poses; richer icons on the game hub.
- README: added measured idle CPU and memory use.

## 0.8.1

- Apps tab: the app picker now looks for new and changed apps every time you open it (the old list stays visible while it updates), and has a Refresh button, so you no longer need to restart Nock to see new apps or fixed icons.

## 0.8.0

- Installer: `NockSetup-<version>.exe` installs Nock just for you (no admin rights), adds it to the Start menu, can start it with Windows, and uninstalls from Windows Settings. The plain `nock.exe` is still published too.
- Apps tab: add apps without editing a file. Click "+ Add app", search your installed apps, click one to pin it, and use the small x to unpin. Shows a spinner while the list loads, with a "Try again" if it takes too long.
- Nock now has its own icon and file details in Explorer instead of the generic Windows icon.
- Nock adds itself to the Start menu so Windows search finds it ("Show in Start menu and search" in the tray menu turns it off).

## 0.7.1

- Controls: removed the Wi-Fi, Bluetooth, Dark mode and Night light tiles (the top bar and Windows' quick settings cover them). The page is now shortcuts: Screenshot, Display, Sound, Network, Power & battery and All settings.

## 0.7.0

- New look for the open panel: a labelled tab row on top (Media, Controls, Today, Stats, Clipboard, Apps, Games, Power) and rounded cards on every page.
- Controls: tiles for Wi-Fi, Bluetooth, Dark mode, Night light, Screenshot and Settings, with their state shown.
- New Stats page: CPU and memory gauges, battery and uptime.
- New Power page: Lock, Sleep, Restart and Shut down (restart and shut down ask for a second click).
- Today: timer, to-dos and calendar in cards.
- Games: fixed text and the character drawing too high after the layout change; tidier corners.

## 0.6.0

- Arcade on the Game tab: Brick Breaker, Slicer, Viking Run and Thor Flight, each with a saved best score.
- Quick capture: press Ctrl+Alt+T anywhere to add a to-do straight from the keyboard.
- Focus timer linked to to-dos: start a session on a task, daily focus and tasks-done stats, optional automatic 5 minute breaks (tray menu).
- Clipboard: pin items (they stay and are saved between runs) and search the history.
- Look-away reminder for your eyes (20-20-20), set from the tray menu.
- The Game tab no longer uses a personal photo.

## 0.5.2

- Media: the notch no longer shows the opposite of what the browser is doing after you pause or resume from a tab (the play/pause state is re-checked shortly after each change).

## 0.5.1

- Top bar volume slider now follows the current default output device immediately (laptop speakers, Bluetooth, monitor).

## 0.5.0

- Mini game on the notch: a click-to-flap game with your photo as the bird (new tab).
- Pill reactions: squish on click, jiggle on hover, a heartbeat when you copy a link, and confetti when the focus timer ends. The pill glows with the music.
- Top bar: clock styles (digital, analog, binary, hours until Friday), an optional CPU and memory read-out, and a water reminder (tray menu).
- Notch: custom focus-timer length (type any number of minutes).
- The photo bobblehead now only appears in the press conference scene.

## 0.4.0

- New scenes: Harry Potter fan art (Harry vs Voldemort, Owl post, Harry and friends vs Voldemort, Quidditch, Dementor); the tray submenu is now just "Scenes".
- Timer: type any length in minutes with the new pencil chip; when it ends, the Quidditch scene plays if there is room.
- Top bar: optional CPU and memory read-out (tray menu, or `stats=1` in `config.ini`).
- Notch: removed the volume and brightness sliders from the Controls page (they live in the top bar).

## 0.3.1

- Scenes: pick a random one each time, never the same twice in a row, and only ones that fit the free space on the bar.

## 0.3.0

- New buddy: a photo bobblehead that bobs and tilts along the bar and speaks its own lines.
- New scene: Press conference. Reporters with mics and a camera crowd round the bobblehead while it answers.
- Fight scenes: stay clear of the notch pill at its widest, start right after the app name on the left, and pick whichever side has more room.

## 0.2.1

- Fight scenes: use the stretch left of the notch when the right side is too narrow; say so when there is no room.

## 0.2.0

- Pixel-art characters (Ronin, Courier, Necromancer) that talk, as buddies and in three more scenes.
- Fight scenes: four original characters (Sparkplug, Cinder, Krag, Veil) act out three short scripted scenes on the bar now and then; tray menu to play or disable.
- Buddy: an animated character on the top bar (kitty, blob, bunny, hero, robot) that strolls, dances to music, follows the pointer and reacts to timers and connections.

- Connecting animation on the Wi-Fi / Bluetooth bar icons, spinner rows and success / failure notes.
- Hover tooltips for Wi-Fi and Bluetooth showing what is connected.
- Join password-protected Wi-Fi networks from the dropdown.

- Notch with music player, clipboard history, launcher, timer, to-dos, calendar and system controls.
- macOS-style top bar with Wi-Fi, Bluetooth, sound, brightness, battery and clock dropdowns.
- Hides in full screen, with a top-edge reveal.
- Light/dark theme, tray icon, start with Windows.
