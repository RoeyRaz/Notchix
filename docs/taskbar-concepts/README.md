# Notchix taskbar concepts

These are visual concepts, not screenshots of a working build.

- `notchix-taskbar-day.png`: one continuous rail for a warm wallpaper.
- `notchix-taskbar-night.png`: connected glass sections for a blue night wallpaper.

## Recommended direction

Use the night concept's connected sections and the day concept's quieter
material. Notchix's direction is a partial-width bar centered at the bottom
edge, with connected glass sections for Start/search, pinned and running apps,
Now Playing, system shortcuts, and time. Its neutral glass samples the
background; purple marks focus, running apps, and small interactions. Album art
colour appears in song text and media controls. Keep the top island independent
so music and transfers remain primary there without duplicating every detail
along the bottom.

The current bar is an early usable shell, not a finished taskbar replacement:
it still needs grouped windows, hover previews, right-click window actions,
keyboard navigation, and the complete notification/tray experience below.

## First usable release

1. Start, search, pinned apps and every running top-level window. Group windows
   from one app and make switching and restoring reliable.
2. Hover window previews and right-click window menus. Keyboard navigation and
   tooltips must work before replacing Explorer's taskbar.
3. System tray and overflow, notifications, audio/network/battery controls,
   clock, calendar and a show-desktop target.
4. Per-monitor placement, work-area reservation, fullscreen/game handoff,
   auto-hide, and immediate restore of the Windows taskbar after exit or crash.
5. Optional compact music state, transfer ring from verified progress only,
   and camera/microphone indicators. Keep these secondary to app switching.

## Release gate

Do not hide the Windows taskbar until items 1-4 are implemented and tested on
Windows 11, including a League match and multiple monitors. Profile idle CPU,
memory and frame pacing before enabling the mode by default. The existing
floating dock is a separate experiment and is not this design.
