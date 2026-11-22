# Nock, Roey edition (experimental source fork)

## Typing Sprint and optional top bar (0.13.16.0)

- Typing Sprint now shows a rolling three-line reading window instead of a
  single sentence. Each test has a long rotating paragraph sequence.
- Choose 30 or 60 seconds from inside the game. New text restarts the run.
- Live WPM, accuracy, a timer and session best remain visible.
- On wider monitors the optional top bar shows transfer context and qBittorrent
  ETA, while the collapsed Island retains its compact circular indicator.
- No new polling threads or always-on taskbar window manager were added.
- Windows build and live interaction testing are still required.

## Camera and microphone privacy lights (0.13.15.0)

- A small orange dot indicates an active microphone capture session; a green
  dot indicates camera access recorded by Windows. Both appear simultaneously.
- Privacy dots share the collapsed Island with music and transfer rings without
  displacing its content, and remain visible during short alerts.
- Camera checks piggyback on an existing two-second timer, with no extra loop.
- The optional top bar already runs mostly on Windows events. A full Windows
  taskbar replacement is deferred until measured performance justifies it.
- Windows runtime validation remains necessary, including camera privacy events.

## Mixed live activities (0.13.14.0)

- Music and its clock stay visible during transfers. A small progress ring on
  the right shows qBittorrent's actual percent and a compact ETA label.
- Unquantified Steam/browser activity receives an animated ring rather than
  an invented percent. When several signals coexist, a count badge appears.
- Alerts and incoming notifications still temporarily take visual priority.
- The original larger transfer display remains when nothing is playing.
- This visual change requires Windows build and runtime testing on the user's PC.

## Ignore streaming and calls (0.13.13.4)

- Browser process I/O alone no longer triggers a Live Activity, preventing
  YouTube, tabs, streaming and cache writes from being mistaken for downloads.
- A browser activity now requires a growing `.part`, `.crdownload` or
  `.download` file in the Windows user's Downloads directory.
- Removed generic Discord, File Explorer, OneDrive and qBittorrent process I/O
  fallback because those signals also cover calls, caches and uploads.
  qBittorrent's authenticated Web API still provides real download status.
- Downloads saved to other directories may not be detected until we add
  browser-specific integrations. Uncertain activity remains hidden.

## Always-visible collapsed island activity bar (0.13.13.3)

- When transfer activity is detected, the compact Island itself replaces the
  media line with an app name and prominent 4px progress strip.
- qBittorrent with Web API configured displays a real progress fraction.
- Steam, browsers and other disk-I/O apps show a smoothly moving indeterminate
  bar instead of a fabricated percentage.
- A short dedicated animation timer runs only while activity is detected and
  the Island is visible; otherwise normal media view resumes.

## Transfer diagnostics (0.13.13.2)

- The Stats > Uptime tile now shows `I/O idle` or the detected active app,
  making the monitor's behavior visible during Steam and browser downloads.
- This is a diagnostic readout of the same counters used in the collapsed
  Island. It does not prove a particular download is being tracked.

## Steam activity detection repair (0.13.13.1)

- Monitor `steamservice.exe` alongside Steam and its web helper processes.
- Detect buffered/asynchronous I/O in addition to disk writes, since Steam
  can continue downloading without immediately writing downloaded bytes.
- Lower the sustained-activity threshold and label the result as I/O activity
  rather than presenting it as an exact network download speed.
- No genuine Steam download percentage is available from this fallback.

## qBittorrent real download status (0.13.13)

- Optional read-only qBittorrent 5.2+ Web API integration.
- Shows the most active download's name, true completion percentage, download
  speed and reported ETA in the compact island, with an actual progress bar.
- The API client only connects to `127.0.0.1` and never saves or logs the API
  key. Activity is optional and falls back to the earlier disk-write monitor.
- See `QBIT_SETUP.md` for one-time setup. No remote WebUI access is required.


## Zen Browser and qBittorrent activity (0.13.12.1)

- Added `zen.exe` and `qbittorrent.exe` to the cross-app disk-write monitor.
- They display their own application labels and measured write rates in the
  compact Island while actively writing files. This still does not expose
  either application's true download completion percentage.

## Cross-app file activity in the island (0.13.12)

- Detects sustained disk writes from Steam, Discord, File Explorer, common
  browsers, OneDrive, Epic Games and Battle.net every two seconds.
- When active, the collapsed island shows the app name, measured write rate,
  and a subtle indeterminate progress strip; notification previews still win.
- This is a best-effort activity indicator, **not** a universal download
  progress reader. It cannot infer transfer totals, exact completion, or
  distinguish cache writes from genuine downloads in every application.
- Does not inspect file content or inject into other applications.

## Camera frame availability fix (0.13.11.1)

- The Mirror now ignores transient null WinRT camera frames rather than
  displaying the misleading `0x00000000` success-code error.
- Camera frame processing resumes as soon as a valid frame arrives.

## Typing Sprint and notification cleanup (0.13.11)

- Tools now contains a native 30-second WPM practice game. Clicking it
  activates keyboard input while the island is open. The timer begins with
  the first keypress, supports Backspace, reports live WPM and accuracy,
  keeps the best result during the current session and offers New sprint.
- Removed the earlier 2-column Tools grid in favor of a tidy 3-by-3 layout
  with all previous shortcuts retained alongside Typing Sprint.
- Activity's existing local-history clear control is now explicitly labeled
  **Remove all**. It does not remove Windows Notification Center messages.
- This is an experimental source update and must be compiled on Windows.

## Minimal island polish (0.13.10.2)

### Activity actions (0.13.10.3)

- Each Activity card has a subtle × dismiss control. It removes only that
  row from Nock's local history, not from Windows Notification Center.
- The footer action is now labeled Clear all, and the heading shows the
  currently visible range when scrolling through notification history.
- After dismissing a row the scroll position is clamped so the list does not
  leave an empty page while there are earlier saved notifications.

- Removed the static pink/green strokes from the idle island perimeter.
- Preserved cover-art-derived accent color but reduced the volume-driven
  glow's opacity and thickness so it is a quiet response, not a light show.
- Removed the decorative circular halo behind the Media album artwork.
- Added dd/mm/yy date text to the idle clock and Home overview.
- Agenda now previews two clearly marked example entries and gives simple
  local iCal connection instructions when no calendars are configured.
- Activity supports wheel scrolling through up to 100 notifications, richer
  app-specific colors and icons. It reuses real shortcut artwork for apps
  present in the Nock Apps list, falling back to meaningful system glyphs.
- Miso retains its contextual expressions and gentle musical-note animation.
- Publishing and rebranding are intentionally deferred.

This is a source-code modification of the MIT-licensed Nock project. It is **not a compiled Windows executable** and has **not been run or compiled** in this environment.

## Double-click installer and updater

After extracting this ZIP, open its `nock-custom` folder and double-click **`INSTALL-OR-UPDATE-NOCK.cmd`**. This is the same entry point for initial installation and later source updates. It builds the release executable using your existing Rust/Windows SDK installation, creates a locally signed MSIX, and installs it. You no longer need to type Cargo or packaging commands manually. The console remains open on failure so you can copy the error.

For subsequent releases, download and extract the new source ZIP and double-click that release's `INSTALL-OR-UPDATE-NOCK.cmd`. The packaging script automatically chooses an MSIX version newer than the currently installed Nock. Your `%APPDATA%\Nock` user settings are not deleted. Close the running Nock first and launch **Nock Custom** from Start after installation. Installing a package does not itself launch the app.

The setup is a Windows CMD/PowerShell launcher, **not a precompiled EXE installer**. A fully automatic online updater would additionally need a stable authenticated update feed and hosted signed release binaries, which this ChatGPT Library ZIP does not provide. The local signing certificate must already be trusted in `LocalMachine\TrustedPeople`, as explained below. Never trust an unfamiliar certificate.

## Design refresh and notification diagnosis

### Neutral glass and inline Shelf

- The expanded shell uses neutral smoked glass and a soft pearl accent instead of a blue-tinted panel. Media artwork can still add its own subtle color.
- Shelf now shows a four-column file collection on the left and an **inline Quick Look** pane on the right. Select a file to see its image, first text lines, or file information without leaving Shelf. Open, Reveal and Copy Path remain available in that pane.
- The Windows notification check now runs every **450 ms** rather than every **1,800 ms**. Enumeration still runs in a non-overlapping background worker. This reduces latency but is not a replacement for a native event subscription and is unverified on a real Windows PC.

### Single guided Windows installation

If you have previously only run `target\release\nock.exe`, the listener has not been given the package identity that Windows requires. In **PowerShell**, from the `nock-custom` folder, run:

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\Install-NockCustom.ps1
```

This builds the signed MSIX using the installed Windows SDK and verifies whether its signing certificate is in **Local Computer → Trusted People**. If it is missing, the script prints an explicit administrator-only import command for you to review and run. The script never imports a certificate automatically. Do not trust a certificate you didn't generate yourself. Close old Nock instances first, then launch **Nock Custom** from the Windows Start menu and select **Activity → Allow access**. This is a local developer package, not a Microsoft Store-signed release. The script never changes Windows or WhatsApp notification preferences. It requires the Rust toolchain, C++ build tools, Windows SDK, and PowerShell's certificate cmdlets.

**If Windows notification banners vanished:** Nock does not change the Windows notification settings. We previously suggested manually turning off banners after integration was working. If you did that, turn them back on for now: Windows Settings → System → Notifications → WhatsApp → **Show notification banners** ON and **Show notifications in notification center** ON. Also enable notifications inside WhatsApp. Test a Windows banner first. Do not disable it again until Nock actually receives messages in Activity.

### Compact island and Activity refresh

- Incoming messages now use a **350 × 58 logical-pixel** island (previously 430 × 78). The content glides in and fades out, then the spring returns to the regular clock/media notch. This is inspired by Apple's compact, minimal and expanded Live Activity hierarchy, not a pixel-art skin.
- Activity itself is now a slim status row and a separate three-message inbox instead of a huge "Notification Island" header.
- On first authorized connection, existing Windows notification-center messages appear in Activity as **history** without triggering stale pop-up animations. Only newly detected messages animate in the notch.
- The optional Preview Island action still tests the visual path without Windows access. The Windows integration must be tested on an installed MSIX on Windows 11. This Linux build workspace cannot validate the Windows runtime.

- Games is removed from the **main navigation** and replaced by **Activity**, which now has a dedicated inbox and a live Windows listener status. The old games code remains in the source, but the tab is no longer shown.
- The visual direction is smooth blue-graphite glass, soft rim lighting, an integrated shortcut dock, and subtle active-tab highlights. No pixel-art redesign.
- The mascot is now **living eyes inside the notch**, not a pale character floating beside it. The eyes blink, follow the pointer, perk up while music plays, and show a quieter expression during focus. `mascot.txt` still controls them.
- Activity has a **Preview Island** button, which generates a local sample notification. **This works in the standalone executable** and isolates UI animation testing from Windows permissions. It is only a demo, not a real WhatsApp message.
- If Activity says **Requires installed MSIX**, you are still running the portable `target\release\nock.exe`. You must build/install the signed MSIX and launch **Nock Custom** from Start to obtain the app identity required by the official notification listener.
- Once the MSIX is installed and access is granted, Activity displays the number of Windows notifications currently visible to the listener or an error returned from Windows.

**If WhatsApp messages do not appear in Windows Notification Center either**, Nock cannot receive them through the Windows listener. First verify Windows **Settings → System → Notifications** are enabled globally and for WhatsApp, that **Do not disturb** is off, and that WhatsApp Desktop's own notifications are enabled. Send a message while WhatsApp is in the background. Only after Windows receives the toast can the listener display it in Nock. Once integration works, you can turn off WhatsApp's Windows *banners* while keeping notifications enabled to make Nock the visible surface.

## Windows notifications and Dynamic Island (experimental)

### Miso, the notch mascot (new)

Miso is a tiny original, softly shaded vector creature that lives **inside the collapsed notch**, independent of the optional top-bar buddy. It blinks, breathes, looks toward the cursor when hovered, and perks up when media is playing. It sits in its own 40-pixel reserved region so album art, the clock, equalizer, and focus countdown keep all of their room. During a full-width Windows notification or important alert, Miso politely gets out of the way. Expanding the panel makes the companion fade with the collapsed view.

Miso is enabled by default. To turn it off without affecting the existing top-bar buddy, create a plain-text file `%APPDATA%\Nock\mascot.txt` containing just `0`, then restart Nock. To turn it back on, delete that file or replace `0` with `1`. Its movement is rendered locally, with no network calls or camera access. The idle animation refreshes the small notch periodically, so disabling it can save a little CPU on low-power laptops.

This edition includes a Windows notification-center listener, a transient animated notch card, a message history in the **Activity** tab, an **Allow access** button, and a **Hide message text** toggle. The listener runs off the UI thread, displays new messages for about six seconds, and does not interrupt Spotify playback buttons. Clicking the collapsed notification opens Activity. The initial connection imports existing messages into Activity without replaying them as popups. History is kept only in memory, not uploaded or written to disk.

**For WhatsApp, Discord, or any other app to work, use the MSIX package, not the standalone `nock.exe`.** Windows notification access requires package identity, the `userNotificationListener` capability, and user approval. The normal executable remains usable but cannot listen to other apps' Windows notifications.

### Package and install on Windows 11

1. Install the Rust MSVC toolchain, Visual Studio C++ Build Tools and Windows 10/11 SDK.
2. In a normal PowerShell window opened in `nock-custom`, run `powershell -ExecutionPolicy Bypass -File .\packaging\windows\Build-NockMsix.ps1`. The script builds `target\release\nock.exe`, creates a signed MSIX under `dist`, and exports a local developer `.cer`. It **does not** install or trust the certificate automatically.
3. Verify you generated the certificate yourself. To trust it on your PC, open the `.cer` and install it for the **Current User → Trusted People** store. Never trust a developer certificate from an unknown source.
4. Close older Nock instances first. Install `dist\Nock-Custom-0.13.3-x64.msix` by double-clicking it or using `Add-AppxPackage` in PowerShell. Launch **Nock Custom** from the Start menu, not by running the unpackaged exe from `target\release`.
5. Open Nock → Home → Activity → **Allow access** and confirm Windows' permission prompt. If the status is Denied, review the system's notification access settings. WhatsApp Desktop must generate Windows notifications, and Windows Notifications must be enabled for it.
6. Send a new test message while WhatsApp is in the background. The new notification should expand the notch into a compact rounded island for approximately six seconds. Click to open the Activity inbox. Existing Windows messages at startup can appear in Activity, but are not replayed as new alerts.

To show an alert **only in Nock**, go to Windows **Settings → System → Notifications → WhatsApp** and turn **Show notification banners** off while leaving WhatsApp notifications enabled. Repeat for other messaging apps as desired. This is an individual Windows setting, not something Nock silently changes. Depending on the app, notifications may still appear in Windows Notification Center. The listener cannot capture a message that WhatsApp never posts as a Windows toast.

**Current limitations:** This is a source implementation awaiting a real Windows compile and integration test. No standalone Windows installer is pre-built in the ZIP. The current UI shows sender and message text when provided by a standard toast binding. It cannot intercept in-app-only messages or guarantee a replacement for Windows' lock-screen or security notifications. Privacy and quiet-alert toggles are in-memory preferences for now.

## Included changes

- Keep the collapsed notch focused on the clock or music instead of replacing it with a persistent `Mic 2m` notice.
- Add **Open on hover** to the Nock tray menu (existing `hover_expand` configuration, now accessible).
- Add a **Microphone** mute/unmute tile to the expanded Controls page.
- Add an **Audio Switch** tile to Controls, using your two known Windows playback devices: `Headset Earphone (G435 Wireless Gaming Headset)` and `Headphones (E-3511)`.
- Show **Spotify album art, track name and the time together** in the collapsed notch.
- Launch audio switching without a terminal window. PowerShell runs only when clicked, with no new always-running worker.
- Add a **Choose audio outputs...** tray command to pick any two Windows playback devices. The preference is saved to `audio-outputs.txt` next to the scripts. The selection window appears only when requested.
- **Experimental v3:** The expanded Media page includes a Spotify-only Windows Volume Mixer slider. It changes the Spotify app session, not the PC's master volume. It does not adjust the volume of a remote Spotify Connect device.
- **Experimental v3:** Stats opens an in-notch Task Manager. It lists the five highest RAM or CPU-consuming processes, refreshes only while open, and requires a second click within four seconds to end a process. Protected process names are blocked, but terminating other apps can still lose unsaved data.
- **Experimental v3:** Calculator runs entirely inside Nock. Open from Controls.
- **Experimental v3:** Quick Capture in Controls uses Nock's built-in local to-do list. It does not yet sync to Notion.
- **Experimental v3:** 'Translucent glass notch' appears in the tray menu. This is a semi-transparent tinted finish, not real wallpaper-aware background blur. It can be turned off and persists in `glass.txt`.
- **Experimental v5:** Spotify mixer volume has a compact mute/restore button and percentage indicator.
- **Experimental v7:** The expanded notch has a Home dashboard showing music with playback controls, time and date, remaining local tasks, CPU/RAM usage, and shortcuts to the main panels. It always opens to Home, including after hover expansion. The typed Quick Commands feature from v6 has been removed as requested.
- **Experimental v8:** Home no longer shows local tasks or a task count. Calculator replaces the Tasks shortcut. The local Today/Tasks tab and Ctrl+Alt+T quick-add hotkey are removed from regular navigation, because Roey keeps actionable tasks in Notion. Old saved tasks are left untouched on disk. The built-in focus timer code remains for compatibility.
- **Experimental v9:** Home has an updated music card with a draggable Spotify app-mixer volume control and mute/restore, improved card borders and spacing, and a next-calendar-event line. Calendar data comes from Nock's existing read-only iCal feed, fetched at startup and approximately hourly, not a new continuously running service.
- **Experimental v10:** Shelf has a **Paste copied files** button. In Explorer, select one or more files and press Ctrl+C, then choose Shelf → Paste copied files. It adds references to the files without moving or copying their bytes. Windows legacy WM_DROPFILES handling is also enabled alongside OLE drag and drop. If OLE target registration fails, Nock writes `shelf-diagnostics.log` in its settings directory. If drag and drop still fails, avoid running Nock as administrator when File Explorer runs normally.
- **Experimental v11:** Multiple read-only calendar feeds can be listed in `calendars.txt` using `Name|Secret iCal URL`, one per line (up to eight additional feeds). The existing single `calendar_url` remains supported as the Main calendar. Nock combines events from all feeds, sorts them by time, shows the next event with a source color on Home, and adds a Calendar tab with the next five events. Refresh runs at startup and approximately hourly, not continuously.
- **Experimental v13:** Calendar lookahead extends from 36 hours to 14 days, and PowerShell explicitly uses UTF-8 for Hebrew event titles. `calendar-diagnostics.log` is written in the Nock settings folder after each refresh, reporting only per-feed success/failure and the count of upcoming timed events. It never includes secret feed URLs, calendar names, or event text. All-day entries still are not supported by this lightweight parser.

Other Fenn plugins, such as the custom multi-calendar agenda, Quick Capture and iPhone Bridge, **are not ported yet**. Some similar functions already exist in stock Nock.

The custom per-monitor taskbar and Alt+Space file search are **planned**, not implemented in this source version. Nock's existing top bar is not yet a multi-monitor taskbar replacement.

## v4 usability update

- Controls has **Task Manager** instead of Quick Capture. The old local to-do database is no longer surfaced in normal navigation.
- The internal Calculator accepts keyboard digits and operators. Enter evaluates, Backspace erases one digit, Esc returns to Controls. Click Calculator once to focus the Nock panel before typing.
- Drag the Spotify volume slider with the mouse. The native Windows Volume Mixer session is changed as you drag.
- Hover over the collapsed notch to open Home (enable **Open on hover** in the tray menu if an existing settings file has it off). Home refreshes CPU/RAM once per second only while visible, and its music controls work without opening the Media tab.
- **Important Spotify distinction:** The current slider is *not* the slider drawn within Spotify Desktop. That separate Spotify Connect playback volume requires an authenticated Spotify integration and a supported playback device. It is not implemented here.

## Build on Windows 11

1. Keep a backup of your installed Nock and `%APPDATA%\Nock` settings. Do not delete them.
2. Install Rust using [rustup](https://rustup.rs/) and the Visual Studio C++ Build Tools with the MSVC toolchain (see upstream `README.md` for project requirements).
3. Open PowerShell in the folder containing `Cargo.toml`, and run `cargo build --release`.
4. Before running the custom build, exit the original Nock from its tray menu so the two versions don't compete for the notch.
5. Copy both `roey-addons\Toggle-Audio.ps1` **and** `roey-addons\Choose-Audio.ps1` into `%APPDATA%\Nock`. You already have the `AudioDeviceCmdlets` PowerShell module installed.
6. Launch the produced executable from `target\release` (check the exact filename produced by Cargo).
7. Right-click its tray icon and select **Open on hover**. The clock stays visible alongside Spotify; expand and visit Controls for **Microphone** and **Audio Switch**. Use **Choose audio outputs...** on the tray menu to pick the two outputs.

### Start with Windows

The tray menu already has **Start with Windows**. It writes the current executable's absolute path to the current user's Windows `Run` registry entry, and Windows launches that file at sign-in. Move/copy your tested `nock.exe` to a permanent folder before enabling this setting. If you later move the executable, turn startup off and on again from its new location. Keep only one Nock autostart entry to avoid two competing instances.

### Show your Google Calendar's next event on Home

1. In Google Calendar on the web, open **Settings**, choose the calendar, then **Integrate calendar**.
2. Copy **Secret address in iCal format** for your own calendar. Keep that link private. Anyone holding it could potentially read events from that calendar.
3. In Nock, right-click the tray icon and select **Open settings folder**. Exit Nock, then edit `config.ini` to set `calendar_url=` followed by the complete iCal URL. Do not send this private URL to anyone.
4. Relaunch Nock. It fetches the feed at startup and about once an hour, then shows the next upcoming event on Home. The feed is read-only and does not create or edit Google events.

The upstream lightweight iCal parser handles simple recurring events but has limitations for some recurrence rules and daylight-saving transitions. Confirm important event times against Google Calendar.

### Add several calendars (v11)

1. Right-click Nock's tray icon → **Open settings folder**. A `calendars.txt` template is created automatically on launch.
2. Exit Nock and open `calendars.txt` in Notepad.
3. Add one entry per calendar, with a display name, vertical bar, and that calendar's private iCal URL. Examples of the format, with placeholders only:

   `College|https://calendar.google.com/calendar/ical/...`

   `Work|https://calendar.google.com/calendar/ical/...`

   `Fitness|https://calendar.google.com/calendar/ical/...`

4. Save the file and restart Nock. The new Calendar tab and Home's time card will use the combined feed. Do not send or publish private calendar URLs.

This does not yet authenticate to Google, write events, or show more than five upcoming events in the Calendar tab. If you previously set `calendar_url=` in `config.ini`, that feed remains active in addition to the ones in `calendars.txt`.

If compiling fails, send the complete error output. This fork has not yet undergone Windows integration testing.

## Use hover today without a custom build

In stock Nock, right-click the tray icon and select **Open settings folder**. Exit Nock, edit `config.ini` and set `hover_expand=1`, then reopen Nock. The stock tray menu does not show this setting. Stock Nock will still show its persistent active-microphone banner until the code is modified.

## Resource usage

This patch does not add an always-running background service. Existing Nock polling remains unchanged. PowerShell starts on demand only when switching audio output, then exits. CPU and RAM usage will never literally be zero while the app is running.
# Visual refresh (0.13.10)

- Activity now uses three distinct graphite notification cards with app-colored
  icon surfaces, separate sender/title and body lines, relative timestamps and
  proper content masking in Private mode.
- Incoming notifications animate in their own island-shaped pill. Multiple
  arrivals are queued (up to eight) instead of overwriting the current card.
- Miso smiles while music plays and releases tiny drifting musical notes,
  rendered as lightweight vector strokes with no additional assets or timers.
- No invasive taskbar replacement or additional always-on background polling.
- Source provenance: this is a modified Nock project originally by Akhilesh
  Varute, distributed under MIT. The existing `LICENSE` must remain included
  when publishing source or distributing a derivative build. A future distinct
  project name, icon and public repository can be chosen without implying this
  is a wholly original codebase. Review third-party assets before publication.

