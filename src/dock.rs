//! Partial-width glass taskbar. Explorer is restored whenever this optional
//! taskbar is hidden or Notchix exits.
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Shell::{SHAppBarMessage, APPBARDATA, ABM_GETSTATE, ABM_SETSTATE, ABS_AUTOHIDE};
use windows::Win32::UI::Input::KeyboardAndMouse::{TRACKMOUSEEVENT, TME_LEAVE, TrackMouseEvent};
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::core::{BOOL, PWSTR};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::app::{App, Page};
use crate::gfx::{Color, Gfx, Rect, Align};
use crate::media::{Cmd, MediaState};
use crate::store::{self, AppEntry};
use crate::sys;

const MAX_W: f32 = 840.0;
const H: f32 = 62.0;
const ICON_STEP: f32 = 48.0;
const TASKBAR_BASE_W: f32 = 480.0;
const MEDIA_W: f32 = 178.0;
const SYSTEM_W: f32 = 168.0;
const HOT_MEDIA: usize = usize::MAX - 1;
const HOT_PREV: usize = usize::MAX - 2;
const HOT_PLAY: usize = usize::MAX - 3;
const HOT_NEXT: usize = usize::MAX - 4;
// TrackMouseEvent posts this message; the windows crate exposes it under
// Win32_UI_Controls, which this lightweight build does not otherwise need.
const WM_MOUSELEAVE: u32 = 0x02A3;

pub struct Dock {
    pub hwnd: HWND,
    gfx: Gfx,
    icons: Vec<Option<ID2D1Bitmap>>,
    running: Vec<bool>,
    extra: Vec<(String, HWND)>,
    extra_icons: HashMap<String, Option<ID2D1Bitmap>>,
    foreground: Option<String>,
    hot: Option<usize>,
    tracking: bool,
    pub enabled: bool,
    glass: crate::glass_capture::GlassCapture,
    glass_bitmap: Option<ID2D1Bitmap>,
    capture_ready: bool,
    previous_taskbar_state: Option<u32>,
}

impl Dock {
    pub fn new(hwnd: HWND, scale: f32, apps: &[AppEntry]) -> Option<Self> {
        // Recover Explorer's prior state if the previous Notchix process
        // ended before its normal shutdown handler could run.
        let marker = store::data_dir().join("taskbar_restore.txt");
        if let Ok(saved) = std::fs::read_to_string(&marker) {
            if let Ok(prior) = saved.trim().parse::<u32>() {
                unsafe {
                    let mut data = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32,
                        lParam: LPARAM(prior as isize), ..Default::default() };
                    SHAppBarMessage(ABM_SETSTATE, &mut data);
                }
            }
            let _ = std::fs::remove_file(&marker);
        }
        let gfx = Gfx::new((MAX_W * scale).ceil() as i32 + 4, (H * scale).ceil() as i32 + 4, scale).ok()?;
        let enabled = std::fs::read_to_string(store::data_dir().join("dock.txt"))
            .map(|v| v.trim() == "1").unwrap_or(false);
        let mut dock = Self { hwnd, gfx, icons: Vec::new(), running: Vec::new(), extra: Vec::new(), extra_icons: HashMap::new(), foreground: None, hot: None, tracking: false, enabled,
            glass: crate::glass_capture::GlassCapture::new(hwnd), glass_bitmap: None, capture_ready: false,
            previous_taskbar_state: None };
        dock.reload_icons(apps);
        Some(dock)
    }

    pub fn reload_icons(&mut self, apps: &[AppEntry]) {
        self.glass_bitmap = None;
        self.running = vec![false; apps.len().min(7)];
        self.icons = apps.iter().take(7).map(|app| {
            let icon = if app.target.starts_with("shell:AppsFolder") {
                sys::shell_icon(&app.target)
            } else {
                sys::resolve_path(&app.target).and_then(|p| sys::file_icon(&p))
            }?;
            let bitmap = self.gfx.bitmap_from_hicon(icon).ok();
            unsafe { let _ = DestroyIcon(icon); }
            bitmap
        }).collect();
    }

    fn icon_rect(i: usize) -> Rect {
        Rect::new(104.0 + i as f32 * ICON_STEP, 11.0, 40.0, 40.0)
    }

    fn item_count(&self, apps: &[AppEntry]) -> usize { apps.len().min(7) + self.extra.len() }
    fn width(&self, apps: &[AppEntry]) -> f32 { TASKBAR_BASE_W + self.item_count(apps) as f32 * ICON_STEP }
    fn extra_slots(pins: usize) -> usize { (((MAX_W - TASKBAR_BASE_W) / ICON_STEP) as usize).saturating_sub(pins.min(7)).min(4) }
    fn media_rect(&self, apps: &[AppEntry]) -> Rect {
        Rect::new(112.0 + self.item_count(apps) as f32 * ICON_STEP, 6.0, MEDIA_W, H - 12.0)
    }
    fn system_rect(w: f32) -> Rect { Rect::new(w - SYSTEM_W - 5.0, 6.0, SYSTEM_W, H - 12.0) }
    fn home_hit(x: f32, y: f32) -> bool { (7.0..49.0).contains(&x) && (6.0..H - 6.0).contains(&y) }
    fn search_hit(x: f32, y: f32) -> bool { (54.0..96.0).contains(&x) && (6.0..H - 6.0).contains(&y) }
    fn clock_hit(x: f32, y: f32, w: f32) -> bool { (w - 67.0..w - 4.0).contains(&x) && (6.0..H - 6.0).contains(&y) }
    fn sound_hit(x: f32, y: f32, w: f32) -> bool { (w - 164.0..w - 140.0).contains(&x) && (6.0..H - 6.0).contains(&y) }
    fn network_hit(x: f32, y: f32, w: f32) -> bool { (w - 139.0..w - 115.0).contains(&x) && (6.0..H - 6.0).contains(&y) }
    fn power_hit(x: f32, y: f32, w: f32) -> bool { (w - 114.0..w - 90.0).contains(&x) && (6.0..H - 6.0).contains(&y) }
    fn notifications_hit(x: f32, y: f32, w: f32) -> bool { (w - 89.0..w - 65.0).contains(&x) && (6.0..H - 6.0).contains(&y) }

    fn media_control(&self, x: f32, y: f32, apps: &[AppEntry]) -> Option<usize> {
        let r = self.media_rect(apps);
        if !(r.y..r.y + r.h).contains(&y) { return None; }
        if (r.x + 114.0..r.x + 134.0).contains(&x) { Some(HOT_PREV) }
        else if (r.x + 134.0..r.x + 157.0).contains(&x) { Some(HOT_PLAY) }
        else if (r.x + 157.0..r.x + r.w).contains(&x) { Some(HOT_NEXT) }
        else if (r.x..r.x + r.w).contains(&x) { Some(HOT_MEDIA) }
        else { None }
    }

    pub fn hit(&self, x: f32, y: f32, apps: &[AppEntry]) -> Option<usize> {
        if !(0.0..H).contains(&y) { return None; }
        (0..apps.len().min(7) + self.extra.len()).find(|i| {
            let r = Self::icon_rect(*i);
            x >= r.x - 8.0 && x <= r.x + r.w + 8.0 && y >= 3.0 && y <= H - 3.0
        })
    }

    fn hover_target(&self, x: f32, y: f32, apps: &[AppEntry], w: f32) -> Option<usize> {
        self.hit(x, y, apps).or_else(|| self.media_control(x, y, apps)).or_else(|| {
            if Self::sound_hit(x, y, w) { Some(usize::MAX - 5) }
            else if Self::network_hit(x, y, w) { Some(usize::MAX - 6) }
            else if Self::power_hit(x, y, w) { Some(usize::MAX - 7) }
            else if Self::notifications_hit(x, y, w) { Some(usize::MAX - 8) }
            else if Self::clock_hit(x, y, w) { Some(usize::MAX - 9) }
            else { None }
        })
    }

    pub fn set_hot(&mut self, item: Option<usize>) -> bool {
        let changed = self.hot != item;
        self.hot = item;
        changed
    }

    pub fn draw(&mut self, apps: &[AppEntry], media: &MediaState, art: Option<&ID2D1Bitmap>, accent: Color, mon: &RECT, scale: f32, refresh_windows: bool) {
        if refresh_windows {
            let windows = visible_apps();
            self.foreground = foreground_exe();
            self.running = apps.iter().take(7).map(|app| pin_exe(app).is_some_and(|name| windows.contains_key(&name))).collect();
            let pinned: HashSet<String> = apps.iter().take(7).filter_map(pin_exe).collect();
            let slots = Self::extra_slots(apps.len());
            let mut extra: Vec<_> = windows.into_iter().filter(|(name, _)| !pinned.contains(name) && !matches!(name.as_str(),
                "nock.exe" | "explorer.exe" | "searchhost.exe" | "startmenuexperiencehost.exe" | "shellexperiencehost.exe")) .collect();
            extra.sort_by(|a, b| a.0.cmp(&b.0));
            extra.truncate(slots);
            self.extra = extra.iter().map(|(name, (hwnd, _))| (name.clone(), *hwnd)).collect();
            self.extra_icons.retain(|name, _| self.extra.iter().any(|(visible, _)| visible == name));
            for (name, (_, path)) in &extra {
                if !self.extra_icons.contains_key(name) {
                    let bitmap = sys::file_icon(Path::new(path)).and_then(|icon| {
                        let result = self.gfx.bitmap_from_hicon(icon).ok();
                        unsafe { let _ = DestroyIcon(icon); }
                        result
                    });
                    self.extra_icons.insert(name.clone(), bitmap);
                }
            }
        }
        let w = self.width(apps);
        let pw = (w * scale).round() as i32;
        let ph = (H * scale).round() as i32;
        if self.gfx.begin(pw, ph).is_err() { return; }
        let g = &self.gfx;
        let (_, month, day_num, _, hh, mm, _, _) = sys::local_time();
        let day = (7..18).contains(&hh);
        let tint = if day { Color(0.76, 0.67, 0.96, 1.0) } else { Color(0.61, 0.66, 1.0, 1.0) };
        let base = if day { Color(0.10, 0.10, 0.13, 0.34) } else { Color(0.045, 0.055, 0.085, 0.38) };
        let shell = Rect::new(1.0, 1.0, w - 2.0, H - 2.0);
        if let Some(bitmap) = &self.glass_bitmap {
            g.draw_bitmap_mapped(bitmap, Rect::new(0.0, 0.0, w, H), shell, 19.0, 0.94);
        }
        g.fill_rrect(shell, 19.0, base);
        g.fill_rrect(shell, 21.0, Color::white(0.025));
        g.stroke_rrect(shell, 21.0, Color::white(0.34), 0.9);
        g.stroke_rrect(Rect::new(shell.x + 1.0, shell.y + 1.0, shell.w - 2.0, shell.h - 2.0), 20.0, Color::white(0.065), 0.8);
        g.line((shell.x + 24.0, 2.0), (shell.x + w - 24.0, 2.0), 0.8, Color::white(0.28));
        g.fill_rrect(Rect::new(shell.x + 24.0, H - 2.5, shell.w - 48.0, 1.0), 0.5, tint.fade(0.24));
        let media_r = self.media_rect(apps);
        let system_r = Self::system_rect(w);
        g.fill_rrect(Rect::new(6.0, 6.0, 91.0, H - 12.0), 17.0, Color::white(0.052));
        if self.item_count(apps) > 0 {
            g.fill_rrect(Rect::new(100.0, 6.0, self.item_count(apps) as f32 * ICON_STEP + 8.0, H - 12.0), 17.0, Color::white(0.035));
        }
        g.fill_rrect(media_r, 17.0, accent.fade(0.065));
        g.stroke_rrect(media_r, 17.0, accent.fade(0.16), 0.65);
        g.fill_rrect(system_r, 17.0, Color::white(0.045));
        for x in [96.0, media_r.x - 5.0, system_r.x - 5.0] {
            g.line((x, 15.0), (x, H - 15.0), 1.0, Color::white(0.15));
        }
        g.icon('\u{E80F}', Rect::new(18.0, 19.0, 24.0, 24.0), &g.fonts.icon14, Color::white(0.96));
        g.icon('\u{E721}', Rect::new(63.0, 19.0, 24.0, 24.0), &g.fonts.icon14, Color::white(0.83));
        for (i, app) in apps.iter().take(7).enumerate() {
            let r = Self::icon_rect(i);
            let active = self.hot == Some(i);
            let focused = pin_exe(app).is_some_and(|name| self.foreground.as_deref() == Some(name.as_str()));
            let draw_r = if active { Rect::new(r.x - 1.5, r.y - 3.0, r.w + 3.0, r.h + 3.0) } else { r };
            if active {
                g.fill_rrect(Rect::new(draw_r.x - 2.0, draw_r.y - 1.0, draw_r.w + 4.0, draw_r.h + 2.0), 15.0, tint.fade(0.10));
                g.fill_rrect(draw_r, 14.0, Color::white(0.12));
                g.stroke_rrect(draw_r, 14.0, tint.fade(0.38), 0.8);
            }
            if focused { g.stroke_rrect(Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0), 12.0, Color::white(0.34), 0.8); }
            if let Some(bmp) = self.icons.get(i).and_then(|b| b.as_ref()) {
                g.draw_bitmap(bmp, Rect::new(draw_r.cx() - 15.0, draw_r.y + 2.0, 30.0, 30.0), 8.0, 1.0);
            } else {
                g.fill_rrect(Rect::new(draw_r.cx() - 15.0, draw_r.y + 2.0, 30.0, 30.0), 9.0, Color::white(0.10));
                let initial = app.name.chars().next().unwrap_or('A').to_string();
                g.text(&initial, Rect::new(draw_r.cx() - 12.0, draw_r.y + 3.0, 24.0, 27.0), &g.fonts.body, Color::white(0.93), Align::Center);
            }
            if self.running.get(i).copied().unwrap_or(false) {
                let width = if focused || active { 20.0 } else { 9.0 };
                let color = if focused { Color::white(0.96) } else { tint.fade(0.96) };
                g.fill_rrect(Rect::new(draw_r.cx() - width / 2.0, H - 9.0, width, 2.5), 1.25, color);
            } else if active {
                g.fill_rrect(Rect::new(draw_r.cx() - 7.0, H - 9.0, 14.0, 2.0), 1.0, Color::white(0.55));
            }
        }
        for (i, (name, _)) in self.extra.iter().enumerate() {
            let index = apps.len().min(7) + i;
            let r = Self::icon_rect(index);
            let active = self.hot == Some(index);
            let focused = self.foreground.as_deref() == Some(name.as_str());
            if active {
                g.fill_rrect(Rect::new(r.x - 2.0, r.y - 1.0, r.w + 4.0, r.h + 2.0), 15.0, tint.fade(0.10));
                g.fill_rrect(r, 14.0, Color::white(0.12));
                g.stroke_rrect(r, 14.0, tint.fade(0.38), 0.8);
            }
            if focused { g.stroke_rrect(Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0), 12.0, Color::white(0.34), 0.8); }
            if let Some(bmp) = self.extra_icons.get(name).and_then(|icon| icon.as_ref()) {
                g.draw_bitmap(bmp, Rect::new(r.cx() - 15.0, r.y + 2.0, 30.0, 30.0), 8.0, 1.0);
            } else {
                g.text(&name.chars().next().unwrap_or('A').to_ascii_uppercase().to_string(), r, &g.fonts.body, Color::white(0.92), Align::Center);
            }
            g.fill_rrect(Rect::new(r.cx() - if focused { 10.0 } else { 5.0 }, H - 9.0, if focused { 20.0 } else { 10.0 }, 2.5), 1.25,
                if focused { Color::white(0.96) } else { tint.fade(0.96) });
        }
        // Compact Now Playing lives between app switching and system tools.
        if let Some(bmp) = art {
            g.draw_bitmap(bmp, Rect::new(media_r.x + 7.0, media_r.y + 7.0, 36.0, 36.0), 9.0, 1.0);
        } else {
            g.fill_rrect(Rect::new(media_r.x + 7.0, media_r.y + 7.0, 36.0, 36.0), 9.0, Color::white(0.075));
            g.icon('\u{E768}', Rect::new(media_r.x + 7.0, media_r.y + 7.0, 36.0, 36.0), &g.fonts.icon14, tint.fade(0.85));
        }
        let title = if media.has_track() { media.title.as_str() } else { "Nothing playing" };
        let artist = if media.has_track() { media.artist.as_str() } else { "Open media" };
        let text_x = media_r.x + 49.0;
        g.text(title, Rect::new(text_x, media_r.y + 7.0, 63.0, 17.0), &g.fonts.tiny, Color::white(0.96), Align::Left);
        g.text(artist, Rect::new(text_x, media_r.y + 23.0, 63.0, 16.0), &g.fonts.tiny, Color::white(0.70), Align::Left);
        let (position, duration) = media.progress(sys::filetime_now());
        if duration > 0.0 {
            g.fill_rrect(Rect::new(text_x, media_r.y + 44.0, 63.0, 2.0), 1.0, Color::white(0.22));
            let progress = (position / duration).clamp(0.0, 1.0) as f32;
            g.fill_rrect(Rect::new(text_x, media_r.y + 44.0, (63.0 * progress).max(1.0), 2.0), 1.0, accent.fade(0.95));
        }
        let control_y = media_r.y + 15.0;
        for (code, rect, hot) in [
            ('\u{E892}', Rect::new(media_r.x + 113.0, control_y + 4.0, 20.0, 23.0), HOT_PREV),
            (if media.playing { '\u{E769}' } else { '\u{E768}' }, Rect::new(media_r.x + 135.0, control_y, 24.0, 30.0), HOT_PLAY),
            ('\u{E893}', Rect::new(media_r.x + 160.0, control_y + 4.0, 16.0, 23.0), HOT_NEXT),
        ] {
            if self.hot == Some(hot) { g.fill_rrect(Rect::new(rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0), 9.0, Color::white(0.12)); }
            g.icon(code, rect, &g.fonts.icon14, if media.has_track() { Color::white(0.91) } else { Color::white(0.38) });
        }
        let iy = system_r.y + 13.0;
        for (glyph, x, hot) in [
            ('\u{E767}', w - 163.0, usize::MAX - 5),
            ('\u{E701}', w - 138.0, usize::MAX - 6),
            ('\u{E850}', w - 113.0, usize::MAX - 7),
            ('\u{E7E7}', w - 88.0, usize::MAX - 8),
        ] {
            if self.hot == Some(hot) { g.fill_circle(x + 10.0, iy + 10.0, 12.0, tint.fade(0.15)); }
            g.icon(glyph, Rect::new(x, iy, 20.0, 20.0), &g.fonts.icon14, Color::white(0.84));
        }
        g.line((w - 63.0, system_r.y + 8.0), (w - 63.0, system_r.y + system_r.h - 8.0), 1.0, Color::white(0.14));
        if self.hot == Some(usize::MAX - 9) { g.fill_rrect(Rect::new(w - 61.0, system_r.y + 3.0, 56.0, system_r.h - 6.0), 12.0, tint.fade(0.12)); }
        g.text(&format!("{hh:02}:{mm:02}"), Rect::new(w - 61.0, system_r.y + 7.0, 56.0, 20.0), &g.fonts.small, Color::white(0.96), Align::Center);
        g.text(&format!("{month:02}/{day_num:02}"), Rect::new(w - 61.0, system_r.y + 26.0, 56.0, 14.0), &g.fonts.tiny, Color::white(0.68), Align::Center);
        let _ = self.gfx.end();
        let x = mon.left + ((mon.right - mon.left) - pw) / 2;
        // Anchor to the bottom edge. Explorer auto-hides while this optional
        // replacement is visible, leaving wallpaper for live background blur.
        let y = mon.bottom - ph - (3.0 * scale).round() as i32;
        let _ = self.gfx.present(self.hwnd, x, y, pw, ph);
        if self.capture_ready { self.glass.request(x, y, pw, ph); }
    }

    fn on_glass_frame(&mut self, apps: &[AppEntry], media: &MediaState, art: Option<&ID2D1Bitmap>, accent: Color, mon: &RECT, scale: f32) {
        let Some(frame) = self.glass.receive() else { return; };
        if !self.enabled || !self.capture_ready { return; }
        if let Some(pixels) = frame.pixels {
            let expected = (self.width(apps) * scale).round() as i32;
            if ((frame.w as i32) * 2 - expected).abs() > 4 { return; }
            if let Ok(bitmap) = self.gfx.bitmap_from_bgra(frame.w, frame.h, &pixels) {
                self.glass_bitmap = Some(bitmap);
                self.draw(apps, media, art, accent, mon, scale, false);
            }
        } else if frame.w == 0 && frame.h == 0 {
            self.glass_bitmap = None;
        }
    }

    fn set_native_taskbar_autohide(&mut self, on: bool) {
        unsafe {
            let mut data = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, ..Default::default() };
            if on && self.previous_taskbar_state.is_none() {
                let prior = SHAppBarMessage(ABM_GETSTATE, &mut data) as u32;
                if prior & ABS_AUTOHIDE == 0 {
                    let _ = std::fs::write(store::data_dir().join("taskbar_restore.txt"), prior.to_string());
                    data.lParam = LPARAM((prior | ABS_AUTOHIDE) as isize);
                    SHAppBarMessage(ABM_SETSTATE, &mut data);
                    self.previous_taskbar_state = Some(prior);
                }
            } else if !on {
                if let Some(prior) = self.previous_taskbar_state.take() {
                    data.lParam = LPARAM(prior as isize);
                    SHAppBarMessage(ABM_SETSTATE, &mut data);
                    let _ = std::fs::remove_file(store::data_dir().join("taskbar_restore.txt"));
                }
            }
        }
    }
}

#[cfg(test)]
mod taskbar_layout_tests {
    use super::Dock;

    #[test]
    fn running_apps_fit_inside_preview_width() {
        for pins in 0..=7 {
            let width = TASKBAR_BASE_W + (pins + Dock::extra_slots(pins)) as f32 * ICON_STEP;
            assert!(width <= MAX_W);
        }
    }
}

fn pin_exe(app: &AppEntry) -> Option<String> {
    let target = app.target.to_ascii_lowercase();
    if target.starts_with("shell:appsfolder") {
        return match app.name.to_ascii_lowercase().as_str() {
            "spotify" => Some("spotify.exe".into()),
            "discord" => Some("discord.exe".into()),
            "whatsapp" => Some("whatsapp.exe".into()),
            "chatgpt" => Some("codex.exe".into()),
            _ => None,
        };
    }
    if !target.ends_with(".exe") { return None; }
    let name = Path::new(&app.target).file_name()?.to_string_lossy().to_ascii_lowercase();
    Some(match name.as_str() {
        "wt.exe" => "windowsterminal.exe".into(),
        "calc.exe" => "calculatorapp.exe".into(),
        _ => name,
    })
}

fn visible_apps() -> HashMap<String, (HWND, String)> {
    let mut windows = HashMap::<String, (HWND, String)>::new();
    unsafe { let _ = EnumWindows(Some(collect_visible_app), LPARAM((&mut windows as *mut HashMap<String, (HWND, String)>) as isize)); }
    windows
}

/// Name of the foreground process, used only for the taskbar's active-app cue.
fn foreground_exe() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() { return None; }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        result.ok()?;
        Path::new(&String::from_utf16_lossy(&path[..len as usize]))
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
    }
}

fn activate_pin(app: &AppEntry) -> bool {
    let Some(exe) = pin_exe(app) else { return false; };
    let Some((hwnd, _)) = visible_apps().get(&exe).cloned() else { return false; };
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
    true
}

unsafe extern "system" fn collect_visible_app(hwnd: HWND, context: LPARAM) -> BOOL {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }
        let mut title = [0u16; 2];
        if GetWindowTextW(hwnd, &mut title) == 0 { return BOOL(1); }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return BOOL(1); };
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        if ok {
            if let Some(name) = Path::new(&String::from_utf16_lossy(&path[..len as usize])).file_name() {
                let windows = &mut *(context.0 as *mut HashMap<String, (HWND, String)>);
                windows.entry(name.to_string_lossy().to_ascii_lowercase()).or_insert((hwnd, String::from_utf16_lossy(&path[..len as usize])));
            }
        }
        BOOL(1)
    }
}

impl App {
    pub fn rebuild_dock(&mut self) {
        let Some(old) = self.dock.take() else { return };
        let enabled = old.enabled;
        let mut old = old;
        old.set_native_taskbar_autohide(false);
        self.dock = Dock::new(old.hwnd, self.scale, &self.apps);
        if let Some(d) = &mut self.dock { d.enabled = enabled; }
        self.sync_dock();
    }

    pub fn is_dock(&self, hwnd: HWND) -> bool {
        self.dock.as_ref().is_some_and(|d| d.hwnd == hwnd)
    }

    pub fn sync_dock(&mut self) {
        let visible = self.dock.as_ref().is_some_and(|d| d.enabled)
            && sys::fullscreen_kind(&[self.hwnd, self.dock.as_ref().map(|d| d.hwnd).unwrap_or_default()], &self.mon) == 0;
        if let Some(d) = &mut self.dock {
            d.set_native_taskbar_autohide(visible);
            unsafe { let _ = ShowWindow(d.hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE }); }
            if visible {
                d.capture_ready = unsafe { SetWindowDisplayAffinity(d.hwnd, WDA_EXCLUDEFROMCAPTURE).is_ok() };
                d.draw(&self.apps, &self.mstate, self.art.as_ref(), self.accent, &self.mon, self.scale, true);
            } else {
                d.glass_bitmap = None;
            }
        }
    }

    /// Redraw only the now-playing strip; don't enumerate every process window
    /// on the one-second playback tick.
    pub fn sync_dock_media(&mut self) {
        let visible = self.dock.as_ref().is_some_and(|d| d.enabled)
            && sys::fullscreen_kind(&[self.hwnd, self.dock.as_ref().map(|d| d.hwnd).unwrap_or_default()], &self.mon) == 0;
        if visible {
            if let Some(d) = &mut self.dock {
                d.draw(&self.apps, &self.mstate, self.art.as_ref(), self.accent, &self.mon, self.scale, false);
            }
        }
    }

    pub fn restore_native_taskbar(&mut self) {
        if let Some(d) = &mut self.dock { d.set_native_taskbar_autohide(false); }
    }

    pub fn toggle_dock(&mut self) {
        if let Some(d) = &mut self.dock {
            d.enabled = !d.enabled;
            let _ = std::fs::write(store::data_dir().join("dock.txt"), if d.enabled { "1" } else { "0" });
        }
        self.sync_dock();
    }

    pub fn dock_msg(&mut self, hwnd: HWND, msg: u32, _wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            crate::glass_capture::WM_GLASS_FRAME => {
                if let Some(d) = &mut self.dock {
                    d.on_glass_frame(&self.apps, &self.mstate, self.art.as_ref(), self.accent, &self.mon, self.scale);
                }
            }
            WM_MOUSEMOVE => {
                let x = (lp.0 as u16 as i16) as f32 / self.scale;
                let y = ((lp.0 >> 16) as u16 as i16) as f32 / self.scale;
                if let Some(d) = &mut self.dock {
                    if !d.tracking {
                        let mut track = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        if unsafe { TrackMouseEvent(&mut track) }.is_ok() { d.tracking = true; }
                    }
                    let width = d.width(&self.apps);
                    let next = d.hover_target(x, y, &self.apps, width);
                    if d.set_hot(next) { d.draw(&self.apps, &self.mstate, self.art.as_ref(), self.accent, &self.mon, self.scale, false); }
                }
            }
            WM_MOUSELEAVE => {
                if let Some(d) = &mut self.dock {
                    d.tracking = false;
                    if d.set_hot(None) { d.draw(&self.apps, &self.mstate, self.art.as_ref(), self.accent, &self.mon, self.scale, false); }
                }
            }
            WM_LBUTTONUP => {
                let x = (lp.0 as u16 as i16) as f32 / self.scale;
                let y = ((lp.0 >> 16) as u16 as i16) as f32 / self.scale;
                if Dock::home_hit(x, y) {
                    sys::open_start_or_search(false);
                    return LRESULT(0);
                }
                if Dock::search_hit(x, y) {
                    sys::open_start_or_search(true);
                    return LRESULT(0);
                }
                let width = self.dock.as_ref().map(|d| d.width(&self.apps)).unwrap_or(TASKBAR_BASE_W);
                if Dock::sound_hit(x, y, width) {
                    self.choose_audio_outputs();
                    return LRESULT(0);
                }
                if Dock::network_hit(x, y, width) {
                    sys::open("ms-settings:network-wifi", "");
                    return LRESULT(0);
                }
                if Dock::power_hit(x, y, width) {
                    sys::open("ms-settings:batterysaver", "");
                    return LRESULT(0);
                }
                if Dock::notifications_hit(x, y, width) {
                    sys::open_notification_center();
                    return LRESULT(0);
                }
                if Dock::clock_hit(x, y, width) {
                    self.set_expanded(true);
                    self.select_page(Page::Agenda);
                    return LRESULT(0);
                }
                let media_control = self.dock.as_ref().and_then(|d| d.media_control(x, y, &self.apps));
                if let Some(action) = media_control {
                    match action {
                        HOT_PREV => { let _ = self.media.tx.send(Cmd::Prev); }
                        HOT_PLAY => { let _ = self.media.tx.send(Cmd::PlayPause); }
                        HOT_NEXT => { let _ = self.media.tx.send(Cmd::Next); }
                        HOT_MEDIA => {
                            self.set_expanded(true);
                            self.select_page(Page::Media);
                        }
                        _ => {}
                    }
                    return LRESULT(0);
                }
                let index = self.dock.as_ref().and_then(|d| d.hit(x, y, &self.apps));
                if let Some(item) = index {
                    if let Some(app) = self.apps.get(item).filter(|_| item < self.apps.len().min(7)) {
                        if !activate_pin(app) { sys::open(&app.target, &app.args); }
                    } else if let Some((_, target)) = self.dock.as_ref().and_then(|d| d.extra.get(item.saturating_sub(self.apps.len().min(7)))) {
                        unsafe { let _ = ShowWindow(*target, SW_RESTORE); let _ = SetForegroundWindow(*target); }
                    }
                }
            }
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            _ => return unsafe { DefWindowProcW(hwnd, msg, _wp, lp) },
        }
        LRESULT(0)
    }
}
