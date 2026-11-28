# Good first issues

Ideas that are well contained and don't need deep knowledge of the code. Pick one, open an issue to say you're on it, and send a pull request.

- **Faster brightness changes.** The slider can feel slow on some laptops (the fallback uses WMI through PowerShell). Look at `src/sys.rs` and see if the direct display-driver path can be used more often, or the change batched / debounced.
- **Add a pixel-art buddy.** Characters are 26x28 pixel grids (`src/pixdata.rs`) drawn in `src/cast.rs`. Design a new one and a few lines of dialogue.
- **New scene.** Scenes are keyframe scripts in `src/scene.rs` (`script()`); the unit tests check they are well formed.
- **New game or a new mode.** The arcade lives in `src/games.rs`: each game has `step`, `click` and `draw`, a tile on the hub, and a saved best score. Mouse only, and short rounds work best.
- **A new Controls shortcut.** The Controls tab is a grid of tiles linking to Windows Settings pages (`SETTINGS_PAGES` in `src/app.rs`, tiles in `page_controls` in `src/ui.rs`).
- **Multi-monitor polish.** Notchix can target another monitor; improve how it handles display changes and scale differences.
- **Signal strength icon** for Wi-Fi in the bar (instead of the plain icon).
- **Bluetooth battery levels** in the dropdown where devices report them.
- **Forget a saved Wi-Fi network / unpair a Bluetooth device** from the dropdowns.
- **Non-English Wi-Fi parsing.** Network info comes from `netsh` output parsed in English.
- **Publish to Scoop.** Add a manifest for Notchix's own release assets.
- **Code signing.** Set up signing for the installer and exe (for example with SignPath or Azure Trusted Signing) to get rid of the SmartScreen warning.
- **Real sounds for the games.** They use simple system beeps today.
