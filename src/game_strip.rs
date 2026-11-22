//! Opt-in, click-through game telemetry strip. It appears only for a foreground
//! League match when PresentMon reports measured frames. The normal island and
//! dock retain their existing game-hide behavior.

use crate::gfx::{Align, Color, Gfx, Rect};
use crate::{presentmon, store, sys};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const W: f32 = 510.0;
const H: f32 = 31.0;

pub struct GameStrip {
    pub hwnd: HWND,
    gfx: Gfx,
    enabled: bool,
    pid: Option<u32>,
    tx: mpsc::Sender<Option<u32>>,
    measured: Arc<Mutex<presentmon::Metrics>>,
    prev_cpu: (u64, u64),
}

impl GameStrip {
    pub fn new(hwnd: HWND, scale: f32) -> Option<Self> {
        let gfx = Gfx::new(
            (W * scale).ceil() as i32 + 2,
            (H * scale).ceil() as i32 + 2,
            scale,
        )
        .ok()?;
        let enabled = std::fs::read_to_string(store::data_dir().join("game_strip.txt"))
            .map(|s| s.trim() == "1")
            .unwrap_or(false);
        let (tx, rx) = mpsc::channel::<Option<u32>>();
        let measured = Arc::new(Mutex::new(presentmon::Metrics::default()));
        let share = Arc::clone(&measured);
        // The worker parks without a game. The service call never blocks the
        // UI thread, and the service connection closes as soon as the match ends.
        std::thread::spawn(move || {
            let mut current = None;
            let mut client: Option<presentmon::PresentMon> = None;
            loop {
                let next = if current.is_none() {
                    rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                } else {
                    rx.recv_timeout(Duration::from_secs(1))
                };
                match next {
                    Ok(pid) if pid != current => {
                        current = pid;
                        client = pid.and_then(presentmon::PresentMon::connect);
                        if let Ok(mut value) = share.lock() {
                            *value = presentmon::Metrics::default();
                        }
                    }
                    Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if let Some(client) = &mut client {
                    let sample = client.sample();
                    if let Ok(mut value) = share.lock() {
                        *value = sample;
                    }
                }
            }
        });
        Some(Self {
            hwnd,
            gfx,
            enabled,
            pid: None,
            tx,
            measured,
            prev_cpu: sys::cpu_times(),
        })
    }

    pub fn is_active(&self) -> bool {
        self.enabled && self.pid.is_some()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        let _ = std::fs::write(
            store::data_dir().join("game_strip.txt"),
            if self.enabled { "1" } else { "0" },
        );
        self.set_game(None);
    }

    pub fn set_game(&mut self, pid: Option<u32>) {
        let pid = if self.enabled { pid } else { None };
        if self.pid == pid {
            return;
        }
        self.pid = pid;
        if let Ok(mut value) = self.measured.lock() {
            *value = presentmon::Metrics::default();
        }
        let _ = self.tx.send(pid);
        self.hide();
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn draw(&mut self, mon: &RECT, scale: f32) {
        if !self.is_active() {
            self.hide();
            return;
        }
        let values = self.measured.lock().map(|v| *v).unwrap_or_default();
        // No PresentMon frames means no strip. This also prevents a launcher
        // window from triggering a misleading game overlay.
        let Some(fps) = values.fps else {
            self.hide();
            return;
        };
        let (idle, total) = sys::cpu_times();
        let dt = total.saturating_sub(self.prev_cpu.1);
        let di = idle.saturating_sub(self.prev_cpu.0);
        self.prev_cpu = (idle, total);
        let cpu = (dt > 0).then_some(((dt.saturating_sub(di)) * 100 / dt).min(100) as u32);
        let ram = sys::mem_percent();
        let (_, _, _, _, hh, mm, _, _) = sys::local_time();
        let pw = (W * scale).round() as i32;
        let ph = (H * scale).round() as i32;
        if self.gfx.begin(pw, ph).is_err() {
            return;
        }
        let g = &self.gfx;
        g.fill_rrect(
            Rect::new(0.5, 0.5, W - 1.0, H - 1.0),
            15.0,
            Color(0.045, 0.055, 0.085, 0.77),
        );
        g.stroke_rrect(
            Rect::new(0.5, 0.5, W - 1.0, H - 1.0),
            15.0,
            Color(0.77, 0.69, 1.0, 0.28),
            0.8,
        );
        let info = format!(
            "FPS {:.0}     CPU {}     GPU {}     RAM {}%     {:02}:{:02}",
            fps,
            cpu.map(|v| format!("{v}%")).unwrap_or_else(|| "--".into()),
            values
                .gpu_percent
                .map(|v| format!("{v:.0}%"))
                .unwrap_or_else(|| "--".into()),
            ram,
            hh,
            mm
        );
        g.text(
            &info,
            Rect::new(15.0, 3.0, W - 30.0, 24.0),
            &g.fonts.small,
            Color::white(0.92),
            Align::Center,
        );
        let _ = self.gfx.end();
        // Top-left, away from League's central scoreboard. Click-through style
        // is set at window creation, so game input never lands in this window.
        let x = mon.left + (18.0 * scale).round() as i32;
        let y = mon.top + (9.0 * scale).round() as i32;
        let _ = self.gfx.present(self.hwnd, x, y, pw, ph);
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }
}

pub fn league_foreground(mon: &RECT) -> Option<u32> {
    use windows::core::PWSTR;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok()?;
        let covers = (rect.left - mon.left).abs() <= 24
            && (rect.top - mon.top).abs() <= 24
            && (rect.right - mon.right).abs() <= 24
            && (rect.bottom - mon.bottom).abs() <= 24;
        if !covers {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let found = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if !found {
            return None;
        }
        let name = std::path::Path::new(&String::from_utf16_lossy(&path[..len as usize]))
            .file_name()?
            .to_string_lossy()
            .to_string();
        is_league_game_exe(&name).then_some(pid)
    }
}

fn is_league_game_exe(name: &str) -> bool {
    name.eq_ignore_ascii_case("League of Legends.exe")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launcher_and_browser_do_not_trigger_game_strip() {
        assert!(is_league_game_exe("League of Legends.exe"));
        assert!(!is_league_game_exe("LeagueClientUx.exe"));
        assert!(!is_league_game_exe("zen.exe"));
    }
}
