//! Adaptive taskbar preview. Windows' taskbar stays available while Start,
//! window grouping, tray and recovery behavior are being implemented.
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
use crate::store::{self, AppEntry};
use crate::sys;

const MAX_W: f32 = 680.0;
const H: f32 = 54.0;
const ICON_STEP: f32 = 52.0;
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
        let mut dock = Self { hwnd, gfx, icons: Vec::new(), running: Vec::new(), extra: Vec::new(), extra_icons: HashMap::new(), hot: None, tracking: false, enabled,
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
        Rect::new(104.0 + i as f32 * ICON_STEP, 7.0, 40.0, 40.0)
    }

    fn width(&self, apps: &[AppEntry]) -> f32 { 258.0 + (apps.len().min(7) + self.extra.len()) as f32 * ICON_STEP }
    fn extra_slots(pins: usize) -> usize { (((MAX_W - 258.0) / ICON_STEP) as usize).saturating_sub(pins.min(7)).min(4) }
    fn home_hit(x: f32, y: f32) -> bool { (7.0..49.0).contains(&x) && (4.0..H - 4.0).contains(&y) }
    fn search_hit(x: f32, y: f32) -> bool { (54.0..96.0).contains(&x) && (4.0..H - 4.0).contains(&y) }
    fn clock_hit(x: f32, y: f32, w: f32) -> bool { (w - 85.0..w - 5.0).contains(&x) && (4.0..H - 4.0).contains(&y) }
    fn sound_hit(x: f32, y: f32, w: f32) -> bool { (w - 127.0..w - 85.0).contains(&x) && (4.0..H - 4.0).contains(&y) }

    pub fn hit(&self, x: f32, y: f32, apps: &[AppEntry]) -> Option<usize> {
        if !(0.0..H).contains(&y) { return None; }
        (0..apps.len().min(7) + self.extra.len()).find(|i| {
            let r = Self::icon_rect(*i);
            x >= r.x - 8.0 && x <= r.x + r.w + 8.0 && y >= 3.0 && y <= H - 3.0
        })
    }

    pub fn set_hot(&mut self, item: Option<usize>) -> bool {
        let changed = self.hot != item;
        self.hot = item;
        changed
    }

    pub fn draw(&mut self, apps: &[AppEntry], mon: &RECT, scale: f32) {
        let windows = visible_apps();
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
        let w = self.width(apps);
        let pw = (w * scale).round() as i32;
        let ph = (H * scale).round() as i32;
        if self.gfx.begin(pw, ph).is_err() { return; }
        let g = &self.gfx;
        let (_, _, _, _, hh, mm, _, _) = sys::local_time();
        let day = (7..18).contains(&hh);
        let base = if day { Color(0.12, 0.11, 0.13, 0.68) } else { Color(0.055, 0.065, 0.09, 0.70) };
        let shell = Rect::new(1.0, 1.0, w - 2.0, H - 2.0);
        if let Some(bitmap) = &self.glass_bitmap {
            g.draw_bitmap_mapped(bitmap, Rect::new(0.0, 0.0, w, H), shell, 19.0, 0.94);
        }
        g.fill_rrect(shell, 19.0, base);
        g.stroke_rrect(shell, 19.0, Color::white(0.26), 0.8);
        g.line((96.0, 13.0), (96.0, H - 13.0), 1.0, Color::white(0.13));
        g.line((w - 137.0, 13.0), (w - 137.0, H - 13.0), 1.0, Color::white(0.13));
        g.icon('\u{E80F}', Rect::new(18.0, 15.0, 24.0, 24.0), &g.fonts.icon14, Color::white(0.94));
        g.icon('\u{E721}', Rect::new(63.0, 15.0, 24.0, 24.0), &g.fonts.icon14, Color::white(0.75));
        for (i, app) in apps.iter().take(7).enumerate() {
            let r = Self::icon_rect(i);
            let active = self.hot == Some(i);
            let draw_r = if active { Rect::new(r.x - 1.5, r.y - 3.0, r.w + 3.0, r.h + 3.0) } else { r };
            if active { g.fill_rrect(draw_r, 14.0, Color::white(0.12)); }
            if let Some(bmp) = self.icons.get(i).and_then(|b| b.as_ref()) {
                g.draw_bitmap(bmp, Rect::new(draw_r.cx() - 15.0, draw_r.y + 2.0, 30.0, 30.0), 8.0, 1.0);
            } else {
                g.fill_rrect(Rect::new(draw_r.cx() - 15.0, draw_r.y + 2.0, 30.0, 30.0), 9.0, Color::white(0.10));
                let initial = app.name.chars().next().unwrap_or('A').to_string();
                g.text(&initial, Rect::new(draw_r.cx() - 12.0, draw_r.y + 3.0, 24.0, 27.0), &g.fonts.body, Color::white(0.93), Align::Center);
            }
            if self.running.get(i).copied().unwrap_or(false) {
                let width = if active { 20.0 } else { 9.0 };
                g.fill_rrect(Rect::new(draw_r.cx() - width / 2.0, H - 9.0, width, 2.5), 1.25, Color(0.82, 0.72, 1.0, 0.96));
            } else if active {
                g.fill_rrect(Rect::new(draw_r.cx() - 7.0, H - 9.0, 14.0, 2.0), 1.0, Color::white(0.55));
            }
        }
        for (i, (name, _)) in self.extra.iter().enumerate() {
            let index = apps.len().min(7) + i;
            let r = Self::icon_rect(index);
            let active = self.hot == Some(index);
            if active { g.fill_rrect(r, 14.0, Color::white(0.12)); }
            if let Some(bmp) = self.extra_icons.get(name).and_then(|icon| icon.as_ref()) {
                g.draw_bitmap(bmp, Rect::new(r.cx() - 15.0, r.y + 2.0, 30.0, 30.0), 8.0, 1.0);
            } else {
                g.text(&name.chars().next().unwrap_or('A').to_ascii_uppercase().to_string(), r, &g.fonts.body, Color::white(0.92), Align::Center);
            }
            g.fill_rrect(Rect::new(r.cx() - 5.0, H - 9.0, 10.0, 2.5), 1.25, Color(0.82, 0.72, 1.0, 0.96));
        }
        g.icon('\u{E767}', Rect::new(w - 119.0, 15.0, 25.0, 24.0), &g.fonts.icon14, Color::white(0.78));
        g.text(&format!("{hh:02}:{mm:02}"), Rect::new(w - 83.0, 7.0, 76.0, 29.0), &g.fonts.body, Color::white(0.95), Align::Center);
        g.text("AGENDA", Rect::new(w - 83.0, 34.0, 76.0, 13.0), &g.fonts.tiny, Color::white(0.55), Align::Center);
        let _ = self.gfx.end();
        let x = mon.left + ((mon.right - mon.left) - pw) / 2;
        // Anchor to the bottom edge. Explorer auto-hides while this opt-in
        // preview is visible, leaving wallpaper for real blur behind it.
        let y = mon.bottom - ph - (3.0 * scale).round() as i32;
        let _ = self.gfx.present(self.hwnd, x, y, pw, ph);
        if self.capture_ready { self.glass.request(x, y, pw, ph); }
    }

    fn on_glass_frame(&mut self, apps: &[AppEntry], mon: &RECT, scale: f32) {
        let Some(frame) = self.glass.receive() else { return; };
        if !self.enabled || !self.capture_ready { return; }
        if let Some(pixels) = frame.pixels {
            let expected = (self.width(apps) * scale).round() as i32;
            if ((frame.w as i32) * 2 - expected).abs() > 4 { return; }
            if let Ok(bitmap) = self.gfx.bitmap_from_bgra(frame.w, frame.h, &pixels) {
                self.glass_bitmap = Some(bitmap);
                self.draw(apps, mon, scale);
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
            let width = 258.0 + (pins + Dock::extra_slots(pins)) as f32 * 52.0;
            assert!(width <= 680.0);
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
                d.draw(&self.apps, &self.mon, self.scale);
            } else {
                d.glass_bitmap = None;
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
                if let Some(d) = &mut self.dock { d.on_glass_frame(&self.apps, &self.mon, self.scale); }
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
                    let next = d.hit(x, y, &self.apps);
                    if d.set_hot(next) { d.draw(&self.apps, &self.mon, self.scale); }
                }
            }
            WM_MOUSELEAVE => {
                if let Some(d) = &mut self.dock {
                    d.tracking = false;
                    if d.set_hot(None) { d.draw(&self.apps, &self.mon, self.scale); }
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
                let width = self.dock.as_ref().map(|d| d.width(&self.apps)).unwrap_or(258.0);
                if Dock::sound_hit(x, y, width) {
                    self.choose_audio_outputs();
                    return LRESULT(0);
                }
                if Dock::clock_hit(x, y, width) {
                    self.set_expanded(true);
                    self.select_page(Page::Agenda);
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
