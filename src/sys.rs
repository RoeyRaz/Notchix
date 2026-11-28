//! Small wrappers around the Windows calls the notch uses for its actions.
//! Everything here runs only in response to a user action or a Windows event.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};

/// On-demand process snapshot for Nock's in-panel task manager.
/// Never polls while the Processes page is closed.
#[derive(Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub ram_mb: u64,
    pub cpu_ticks: u64,
    pub cpu_pct: f32,
}

pub fn process_snapshot() -> Vec<ProcessInfo> {
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::System::ProcessStatus::*;
    use windows::Win32::System::Threading::*;
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let pid = entry.th32ProcessID;
                let end = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                if pid > 4 && !name.is_empty() {
                    let mut item = ProcessInfo { pid, name, ram_mb: 0, cpu_ticks: 0, cpu_pct: 0.0 };
                    if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid) {
                        let mut mem = PROCESS_MEMORY_COUNTERS::default();
                        if K32GetProcessMemoryInfo(handle, &mut mem, std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32).as_bool() {
                            item.ram_mb = mem.WorkingSetSize as u64 / 1_048_576;
                        }
                        let (mut c, mut x, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
                        if GetProcessTimes(handle, &mut c, &mut x, &mut k, &mut u).is_ok() {
                            let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
                            item.cpu_ticks = ticks(k).saturating_add(ticks(u));
                        }
                        let _ = CloseHandle(handle);
                    }
                    out.push(item);
                }
                if Process32NextW(snap, &mut entry).is_err() { break; }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// Deliberately blocks critical Windows processes and Nock itself. A separate
/// user confirmation is also required in the UI before this is called.
pub fn end_user_process(pid: u32, expected_name: &str) -> std::result::Result<(), String> {
    use windows::Win32::System::Threading::*;
    let protected = ["system", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "svchost.exe", "dwm.exe", "explorer.exe", "nock.exe"];
    if pid <= 4 || protected.iter().any(|n| expected_name.eq_ignore_ascii_case(n)) {
        return Err("Protected Windows or Nock process".into());
    }
    if !process_snapshot().iter().any(|p| p.pid == pid && p.name.eq_ignore_ascii_case(expected_name)) {
        return Err("Process changed or already exited".into());
    }
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|_| "Could not open process (permission denied)".to_string())?;
        let result = TerminateProcess(h, 1).map_err(|_| "Could not end process (permission denied)".to_string());
        let _ = CloseHandle(h);
        result
    }
}
use windows::Win32::Media::Audio::*;
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTimePreciseAsFileTime};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Posted when the background brightness query finished.
/// WPARAM = 1 if supported, LPARAM = percent.
pub const WM_BRIGHT: u32 = 0x8000 + 2; // WM_APP + 2

const CF_UNICODETEXT: u32 = 13;

// ------------------------------------------------------------------ volume

thread_local! {
    static ENUMERATOR: RefCell<Option<IMMDeviceEnumerator>> = const { RefCell::new(None) };
    static ENDPOINT: RefCell<Option<(IAudioEndpointVolume, String)>> = const { RefCell::new(None) };
}

/// The volume control of whatever output device is the default *right now*. The
/// default is looked up on every call (it is cheap) and the control is re-bound
/// the moment it changes, so switching between laptop speakers, Bluetooth or a
/// monitor is followed immediately.
fn endpoint() -> Option<IAudioEndpointVolume> {
    unsafe {
        let en = ENUMERATOR.with(|e| {
            if e.borrow().is_none() {
                *e.borrow_mut() = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok();
            }
            e.borrow().clone()
        })?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        let id_ptr = dev.GetId().ok()?;
        let id = id_ptr.to_string().unwrap_or_default();
        windows::Win32::System::Com::CoTaskMemFree(Some(id_ptr.0 as *const _));
        ENDPOINT.with(|e| {
            let same = e.borrow().as_ref().map(|(_, cur)| *cur == id).unwrap_or(false);
            if !same {
                let ep = dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()?;
                *e.borrow_mut() = Some((ep, id));
            }
            e.borrow().as_ref().map(|(ep, _)| ep.clone())
        })
    }
}

fn drop_endpoint() {
    ENDPOINT.with(|e| *e.borrow_mut() = None);
    ENUMERATOR.with(|e| *e.borrow_mut() = None);
}

/// (volume 0..1, muted)
pub fn get_volume() -> Option<(f32, bool)> {
    let ep = endpoint()?;
    unsafe {
        let v = ep.GetMasterVolumeLevelScalar();
        let m = ep.GetMute();
        match (v, m) {
            (Ok(v), Ok(m)) => Some((v, m.as_bool())),
            _ => {
                drop_endpoint(); // default device probably changed
                None
            }
        }
    }
}

/// Opaque identity of the current playback endpoint, for change notifications.
/// Never displays or stores the identifier outside this process.
pub fn output_device_id() -> Option<String> {
    endpoint()?;
    ENDPOINT.with(|cache| cache.borrow().as_ref().map(|(_, id)| id.clone()))
}

pub fn set_volume(v: f32) {
    if let Some(ep) = endpoint() {
        unsafe {
            if ep
                .SetMasterVolumeLevelScalar(v.clamp(0.0, 1.0), std::ptr::null())
                .is_err()
            {
                drop_endpoint();
            }
        }
    }
}

/// Arrange the currently active normal application in the monitor work area.
/// Uses Windows positioning APIs, so no background hooks are required.
pub fn snap_foreground(layout: u8, notch: HWND) -> bool {
    unsafe {
        let target = GetForegroundWindow();
        if target.is_invalid() || target == notch || !IsWindowVisible(target).as_bool() { return false; }
        if matches!(class_of(target).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") { return false; }
        let monitor = MonitorFromWindow(target, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() { return false; }
        let r = info.rcWork;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let (x, y, ww, hh) = match layout {
            0 => (r.left, r.top, w / 2, h),
            1 => (r.left + w / 2, r.top, w - w / 2, h),
            2 => (r.left, r.top, w, h),
            3 => (r.left, r.top, w / 2, h / 2),
            4 => (r.left + w / 2, r.top, w - w / 2, h / 2),
            5 => (r.left, r.top + h / 2, w / 2, h - h / 2),
            6 => (r.left + w / 2, r.top + h / 2, w - w / 2, h - h / 2),
            7 => (r.left, r.top, w / 3, h),
            8 => (r.left + w / 3, r.top, w / 3, h),
            9 => (r.left + 2 * (w / 3), r.top, w - 2 * (w / 3), h),
            _ => return false,
        };
        let _ = ShowWindow(target, SW_RESTORE);
        SetWindowPos(target, None, x, y, ww.max(1), hh.max(1), SWP_NOZORDER | SWP_NOACTIVATE).is_ok()
    }
}

/// Let Windows 11 show its own native visual snap-layout picker for the
/// foreground application. Windows also handles drag-to-edge snapping itself.
pub fn open_windows_snap_layouts() {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    unsafe {
        let key = |vk: u16, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk), wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0, dwExtraInfo: 0,
            } },
        };
        let input = [key(0x5B, false), key(0x5A, false), key(0x5A, true), key(0x5B, true)];
        SendInput(&input, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Windows Volume Mixer level for Spotify.exe on the current default output.
/// This is Spotify's per-app volume, not the system master volume or Spotify
/// Connect volume on another device.
fn with_spotify_session<T>(f: impl FnOnce(ISimpleAudioVolume) -> Option<T>) -> Option<T> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        let manager: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None).ok()?;
        let sessions = manager.GetSessionEnumerator().ok()?;
        let count = sessions.GetCount().ok()?;
        // No memory or CPU sampling here: querying a volume must stay cheap.
        use windows::Win32::System::Diagnostics::ToolHelp::*;
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut spotify_pids = Vec::new();
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let end = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
                if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case("Spotify.exe") {
                    spotify_pids.push(entry.th32ProcessID);
                }
                if Process32NextW(snap, &mut entry).is_err() { break; }
            }
        }
        let _ = CloseHandle(snap);
        for i in 0..count {
            let Ok(session) = sessions.GetSession(i) else { continue };
            let Ok(control) = session.cast::<IAudioSessionControl2>() else { continue };
            let Ok(pid) = control.GetProcessId() else { continue };
            if spotify_pids.contains(&pid) {
                if let Ok(vol) = session.cast::<ISimpleAudioVolume>() {
                    return f(vol);
                }
            }
        }
        None
    }
}

pub fn spotify_volume() -> Option<f32> {
    with_spotify_session(|vol| unsafe { vol.GetMasterVolume().ok() })
}

pub fn set_spotify_volume(v: f32) -> bool {
    with_spotify_session(|vol| unsafe {
        vol.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null()).ok()?;
        Some(())
    }).is_some()
}

pub fn set_mute(m: bool) {
    if let Some(ep) = endpoint() {
        unsafe {
            let _ = ep.SetMute(m, std::ptr::null());
        }
    }
}

/// Default microphone: current mute state, or flip it. Returns the new state.
pub fn mic_muted() -> bool {
    mic_endpoint().and_then(|ep| unsafe { ep.GetMute().ok() }).is_some_and(|m| m.as_bool())
}

pub fn toggle_mic_mute() -> bool {
    let now = !mic_muted();
    if let Some(ep) = mic_endpoint() {
        unsafe {
            let _ = ep.SetMute(now, std::ptr::null());
        }
    }
    now
}

fn mic_endpoint() -> Option<IAudioEndpointVolume> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(eCapture, eConsole).ok()?;
        dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
    }
}

// -------------------------------------------------------------- brightness
// Internal laptop panels are only reachable through WMI. Spawning PowerShell
// is slow (~0.3 s) but only happens on open / on slider release, never idle.

pub(crate) fn powershell(cmd: &str) -> Option<String> {
    use std::process::{Command, Stdio};
    let mut c = Command::new("powershell.exe");
    // -EncodedCommand (base64 of UTF-16) sidesteps every quoting problem.
    let utf16: Vec<u8> = cmd.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let encoded = base64(&utf16);
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &encoded,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = match c.output() {
        Ok(o) => o,
        Err(e) => {
            log_brightness(&format!("could not start powershell: {e}"));
            return None;
        }
    };
    if !out.status.success() {
        log_brightness(&format!(
            "powershell failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
        return None;
    }
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        log_brightness(&format!("powershell warning: {}", err.trim()));
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Brightness problems are written to %APPDATA%\Notch\brightness.log.
/// Press and release a media key (play/pause 0xB3, next 0xB0, previous 0xB1).
pub fn media_key(vk: u16) {
    unsafe {
        let key = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        SendInput(&[key(false), key(true)], std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn log_media(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::store::data_dir().join("media.log")) {
        let _ = writeln!(f, "{msg}");
    }
}

fn log_brightness(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::store::data_dir().join("brightness.log"))
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for ch in data.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if ch.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// Native path: the display driver's own IOCTLs on \\.\LCD. Takes a millisecond, no
// process to start. Works for built-in laptop panels; anything else falls back to
// the slower WMI route below.

const IOCTL_SUPPORTED: u32 = 0x0023_0406;
const IOCTL_QUERY: u32 = 0x0023_0408;
const IOCTL_SET: u32 = 0x0023_040C;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct DisplayBrightness {
    policy: u8, // 1 = AC, 2 = DC, 3 = both
    ac: u8,
    dc: u8,
}

fn lcd_open() -> Option<HANDLE> {
    use windows::Win32::Storage::FileSystem::*;
    unsafe {
        CreateFileW(
            w!("\\\\.\\LCD"),
            0xC000_0000, // GENERIC_READ | GENERIC_WRITE
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()
    }
}

fn lcd_levels(h: HANDLE) -> Vec<u8> {
    use windows::Win32::System::IO::DeviceIoControl;
    let mut buf = [0u8; 256];
    let mut n = 0u32;
    unsafe {
        if DeviceIoControl(
            h,
            IOCTL_SUPPORTED,
            None,
            0,
            Some(buf.as_mut_ptr() as *mut _),
            buf.len() as u32,
            Some(&mut n),
            None,
        )
        .is_err()
        {
            return Vec::new();
        }
    }
    buf[..(n as usize).min(256)].to_vec()
}

fn native_brightness_get() -> Option<u32> {
    use windows::Win32::System::IO::DeviceIoControl;
    let h = lcd_open()?;
    let mut b = DisplayBrightness::default();
    let mut n = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            h,
            IOCTL_QUERY,
            None,
            0,
            Some(&mut b as *mut _ as *mut _),
            std::mem::size_of::<DisplayBrightness>() as u32,
            Some(&mut n),
            None,
        )
        .is_ok()
    };
    unsafe {
        let _ = CloseHandle(h);
    }
    if !ok {
        return None;
    }
    // On battery the DC level is the live one.
    let on_ac = battery_info().map(|x| x.plugged).unwrap_or(true);
    Some(if on_ac { b.ac } else { b.dc } as u32)
}

fn native_brightness_set(percent: u32) -> bool {
    use windows::Win32::System::IO::DeviceIoControl;
    let Some(h) = lcd_open() else { return false };
    let levels = lcd_levels(h);
    // The driver only accepts its own levels, so snap to the nearest one.
    let want = percent.min(100) as i32;
    let v = levels
        .iter()
        .copied()
        .min_by_key(|l| (*l as i32 - want).abs())
        .unwrap_or(want as u8);
    let b = DisplayBrightness { policy: 3, ac: v, dc: v };
    let mut n = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            h,
            IOCTL_SET,
            Some(&b as *const _ as *const _),
            std::mem::size_of::<DisplayBrightness>() as u32,
            None,
            0,
            Some(&mut n),
            None,
        )
        .is_ok()
    };
    unsafe {
        let _ = CloseHandle(h);
    }
    if !ok {
        log_brightness("native brightness set failed, falling back to WMI");
    }
    ok
}

pub fn brightness_load(hwnd: isize) {
    std::thread::spawn(move || {
        let v = native_brightness_get().or_else(|| {
            powershell(
                "$b = (Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction SilentlyContinue | Select-Object -First 1).CurrentBrightness; if ($null -eq $b) { $b = (Get-WmiObject -Namespace root/WMI -Class WmiMonitorBrightness -ErrorAction SilentlyContinue | Select-Object -First 1).CurrentBrightness }; Write-Output $b",
            )
            .and_then(|s| {
                let t = s.trim().to_string();
                let p = t.parse::<u32>().ok();
                if p.is_none() {
                    log_brightness(&format!("brightness query returned: {:?}", t));
                }
                p
            })
        });
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_BRIGHT,
                WPARAM(v.is_some() as usize),
                LPARAM(v.unwrap_or(0) as isize),
            );
        }
    });
}

static BRI_PENDING: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static BRI_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set the brightness. Safe to call many times while dragging: calls are coalesced
/// so only the latest value is applied, one at a time.
pub fn brightness_set(percent: u32) {
    use std::sync::atomic::Ordering::SeqCst;
    BRI_PENDING.store(percent.min(100) as i32, SeqCst);
    if BRI_RUNNING.swap(true, SeqCst) {
        return;
    }
    std::thread::spawn(|| loop {
        let v = BRI_PENDING.swap(-1, SeqCst);
        if v >= 0 {
            if !native_brightness_set(v as u32) {
                let _ = powershell(&format!(
                    "Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods | Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{Timeout=1;Brightness={}}}",
                    v
                ));
            }
            continue;
        }
        BRI_RUNNING.store(false, SeqCst);
        // a call may have slipped in between the swap and the store
        if BRI_PENDING.load(SeqCst) >= 0 && !BRI_RUNNING.swap(true, SeqCst) {
            continue;
        }
        break;
    });
}

// ------------------------------------------------------- status bar helpers

/// True when Windows is set to the light theme for the system UI (taskbar etc.).
pub fn light_theme() -> bool {
    unsafe {
        let mut v = 0u32;
        let mut n = 4u32;
        let r = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut n),
        );
        r == ERROR_SUCCESS && v == 1
    }
}

/// True while some app is using the microphone (Windows records it in the privacy
/// consent store: an app that is recording has a start time and no stop time yet).
pub fn mic_in_use() -> bool {
    mic_session_active() || mic_in_use_registry()
}

/// True when some other process has an active capture stream on a default microphone.
/// Unlike the registry log this is live, so it also sees apps like Sound Recorder.
fn mic_session_active() -> bool {
    use windows::Win32::Media::Audio::{AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2};
    let pid = std::process::id();
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return false };
        for role in [eConsole, eCommunications] {
            let Ok(dev) = en.GetDefaultAudioEndpoint(eCapture, role) else { continue };
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(list) = mgr.GetSessionEnumerator() else { continue };
            for i in 0..list.GetCount().unwrap_or(0) {
                let Ok(c) = list.GetSession(i) else { continue };
                if c.GetState().ok() != Some(AudioSessionStateActive) {
                    continue;
                }
                if let Ok(c2) = c.cast::<IAudioSessionControl2>() {
                    if c2.GetProcessId().unwrap_or(0) != pid && c2.IsSystemSoundsSession() != S_OK {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn mic_in_use_registry() -> bool {
    capability_in_use_registry("microphone")
}

/// Windows privacy consent records an active camera stream with an unset stop time.
/// Read only, sampled at the already-existing slow transfer timer.
pub fn camera_in_use() -> bool {
    capability_in_use_registry("webcam")
}

fn capability_in_use_registry(capability: &str) -> bool {
    let base = format!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{capability}");
    let qword = |path: &str, name: PCWSTR| -> Option<u64> {
        let mut v = 0u64;
        let mut n = 8u32;
        let p = HSTRING::from(path);
        let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, &p, name, RRF_RT_REG_QWORD, None, Some(&mut v as *mut u64 as *mut _), Some(&mut n)) };
        (r == ERROR_SUCCESS).then_some(v)
    };
    let subkeys = |path: &str| -> Vec<String> {
        let mut out = Vec::new();
        let p = HSTRING::from(path);
        unsafe {
            let mut k = HKEY::default();
            if RegOpenKeyExW(HKEY_CURRENT_USER, &p, None, KEY_READ, &mut k) != ERROR_SUCCESS {
                return out;
            }
            for i in 0.. {
                let mut buf = [0u16; 512];
                let mut n = buf.len() as u32;
                if RegEnumKeyExW(k, i, Some(PWSTR(buf.as_mut_ptr())), &mut n, None, None, None, None) != ERROR_SUCCESS {
                    break;
                }
                out.push(String::from_utf16_lossy(&buf[..n as usize]));
            }
            let _ = RegCloseKey(k);
        }
        out
    };
    let mut paths: Vec<String> = Vec::new();
    for name in subkeys(&base) {
        if name == "NonPackaged" {
            paths.extend(subkeys(&format!("{base}\\NonPackaged")).into_iter().map(|n| format!("{base}\\NonPackaged\\{n}")));
        } else {
            paths.push(format!("{base}\\{name}"));
        }
    }
    paths.iter().any(|p| qword(p, w!("LastUsedTimeStart")).unwrap_or(0) != 0 && qword(p, w!("LastUsedTimeStop")) == Some(0))
}

pub const WM_MIC: u32 = 0x8000 + 11; // WM_APP + 11

const MIC_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

/// Post WM_MIC whenever Windows changes the microphone consent store (an app starting or
/// stopping recording). The thread sleeps inside the registry wait, so it costs nothing idle.
pub fn watch_mic(hwnd: isize) {
    std::thread::spawn(move || loop {
        unsafe {
            let mut k = HKEY::default();
            let key = HSTRING::from(MIC_KEY);
            if RegOpenKeyExW(HKEY_CURRENT_USER, &key, None, KEY_NOTIFY, &mut k) != ERROR_SUCCESS {
                std::thread::sleep(std::time::Duration::from_secs(30)); // no app has used the mic yet
                continue;
            }
            let r = RegNotifyChangeKeyValue(k, true, REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME, None, false);
            let _ = RegCloseKey(k);
            if r == ERROR_SUCCESS {
                let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_MIC, WPARAM(0), LPARAM(0));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300)); // one burst of writes = one update
    });
}

/// Tells the window when a microphone session appears or starts/stops capturing.
#[windows::core::implement(windows::Win32::Media::Audio::IAudioSessionEvents)]
struct MicEvents(isize);

impl windows::Win32::Media::Audio::IAudioSessionEvents_Impl for MicEvents_Impl {
    fn OnDisplayNameChanged(&self, _: &PCWSTR, _: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnIconPathChanged(&self, _: &PCWSTR, _: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnSimpleVolumeChanged(&self, _: f32, _: BOOL, _: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnChannelVolumeChanged(&self, _: u32, _: *const f32, _: u32, _: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnGroupingParamChanged(&self, _: *const GUID, _: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnStateChanged(&self, _: windows::Win32::Media::Audio::AudioSessionState) -> Result<()> {
        post_mic(self.0);
        Ok(())
    }
    fn OnSessionDisconnected(&self, _: windows::Win32::Media::Audio::AudioSessionDisconnectReason) -> Result<()> {
        post_mic(self.0);
        Ok(())
    }
}

#[windows::core::implement(windows::Win32::Media::Audio::IAudioSessionNotification)]
struct MicNotify(isize);

impl windows::Win32::Media::Audio::IAudioSessionNotification_Impl for MicNotify_Impl {
    fn OnSessionCreated(&self, new: Ref<windows::Win32::Media::Audio::IAudioSessionControl>) -> Result<()> {
        if let Some(c) = new.as_ref() {
            watch_session(c, self.0);
        }
        post_mic(self.0);
        Ok(())
    }
}

fn post_mic(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_MIC, WPARAM(0), LPARAM(0));
    }
}

fn watch_session(c: &windows::Win32::Media::Audio::IAudioSessionControl, hwnd: isize) {
    use windows::Win32::Media::Audio::IAudioSessionEvents;
    let ev: IAudioSessionEvents = MicEvents(hwnd).into();
    unsafe {
        let _ = c.RegisterAudioSessionNotification(&ev);
    }
    // the endpoint keeps the registration alive only while we hold both; leak them (a few per run)
    std::mem::forget(ev);
    std::mem::forget(c.clone());
}

/// Event-driven live mic watcher: new capture sessions and their start/stop post WM_MIC.
/// Runs on its own COM thread and sleeps; costs nothing while idle.
pub fn watch_mic_sessions(hwnd: isize) {
    std::thread::spawn(move || unsafe {
        use windows::Win32::Media::Audio::{IAudioSessionManager2, IAudioSessionNotification};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return };
        let mut keep = Vec::new();
        for role in [eConsole, eCommunications] {
            let Ok(dev) = en.GetDefaultAudioEndpoint(eCapture, role) else { continue };
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let n: IAudioSessionNotification = MicNotify(hwnd).into();
            let _ = mgr.RegisterSessionNotification(&n);
            if let Ok(list) = mgr.GetSessionEnumerator() {
                for i in 0..list.GetCount().unwrap_or(0) {
                    if let Ok(c) = list.GetSession(i) {
                        watch_session(&c, hwnd);
                    }
                }
            }
            keep.push((mgr, n));
        }
        loop {
            std::thread::park();
        }
    });
}

pub struct BatteryInfo {
    pub percent: u8,
    pub charging: bool,
    pub plugged: bool,
    pub secs_left: Option<u32>,
    pub saver: bool,
}

/// None on machines without a battery.
pub fn battery_info() -> Option<BatteryInfo> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    unsafe {
        let mut s = SYSTEM_POWER_STATUS::default();
        GetSystemPowerStatus(&mut s).ok()?;
        if s.BatteryFlag & 128 != 0 || s.BatteryLifePercent > 100 {
            return None;
        }
        Some(BatteryInfo {
            percent: s.BatteryLifePercent,
            charging: s.BatteryFlag & 8 != 0,
            plugged: s.ACLineStatus == 1,
            secs_left: (s.BatteryLifeTime != u32::MAX).then_some(s.BatteryLifeTime),
            saver: s.SystemStatusFlag == 1,
        })
    }
}

/// Seconds since the last keyboard or mouse input.
pub fn idle_secs() -> u32 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    unsafe {
        let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if !GetLastInputInfo(&mut li).as_bool() {
            return 0;
        }
        GetTickCount().wrapping_sub(li.dwTime) / 1000
    }
}

/// (percent, charging) or None on machines without a battery.
pub fn battery() -> Option<(u8, bool)> {
    battery_info().map(|b| (b.percent, b.charging || b.plugged))
}

/// Posted when the Wi-Fi name lookup finished.
pub const WM_POP: u32 = 0x8000 + 5; // WM_APP + 5

#[derive(Clone, Default)]
pub struct WifiNet {
    pub ssid: String,
    pub signal: u32,
    pub secure: bool,
    pub saved: bool,
    /// Authentication as reported by Windows, e.g. "WPA2-Personal".
    pub auth: String,
}

#[derive(Clone, Default)]
pub struct WifiState {
    pub connected: Option<String>,
    pub nets: Vec<WifiNet>,
    /// Network we are currently trying to join.
    pub busy: Option<String>,
    /// Outcome of the last connect attempt (text, success); taken by the UI.
    pub last: Option<(String, bool)>,
}

pub static WIFI: std::sync::Mutex<WifiState> = std::sync::Mutex::new(WifiState {
    connected: None,
    nets: Vec::new(),
    busy: None,
    last: None,
});

/// Run `netsh` with a raw argument string (so quotes around names survive).
fn netsh(args: &str) -> String {
    use std::process::{Command, Stdio};
    let mut c = Command::new("netsh");
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.raw_arg(args);
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        c.args(args.split_whitespace());
    }
    match c.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}

fn value_after_colon(l: &str) -> Option<&str> {
    l.split_once(':').map(|(_, v)| v.trim())
}

fn wifi_scan_blocking() -> (Option<String>, Vec<WifiNet>) {
    // connected network
    let mut connected = None;
    for line in netsh("wlan show interfaces").lines() {
        let l = line.trim();
        if l.starts_with("SSID") && !l.starts_with("BSSID") {
            if let Some(v) = value_after_colon(l) {
                if !v.is_empty() {
                    connected = Some(v.to_string());
                }
            }
            break;
        }
    }
    // saved profiles ("All User Profile     : Name")
    let mut saved: Vec<String> = Vec::new();
    for line in netsh("wlan show profiles").lines() {
        let l = line.trim();
        if l.contains("Profile") && l.contains(':') {
            if let Some(v) = value_after_colon(l) {
                if !v.is_empty() {
                    saved.push(v.to_string());
                }
            }
        }
    }
    // visible networks
    let mut nets: Vec<WifiNet> = Vec::new();
    let mut cur: Option<WifiNet> = None;
    for line in netsh("wlan show networks mode=bssid").lines() {
        let l = line.trim();
        if l.starts_with("SSID ") {
            if let Some(n) = cur.take() {
                nets.push(n);
            }
            let name = value_after_colon(l).unwrap_or("").to_string();
            cur = Some(WifiNet {
                saved: saved.iter().any(|s| *s == name),
                ssid: name,
                ..Default::default()
            });
        } else if let Some(n) = cur.as_mut() {
            if l.starts_with("Authentication") {
                let a = value_after_colon(l).unwrap_or("");
                n.auth = a.to_string();
                n.secure = !a.eq_ignore_ascii_case("Open");
            } else if l.starts_with("Signal") {
                let pct = value_after_colon(l)
                    .unwrap_or("")
                    .trim_end_matches('%')
                    .trim()
                    .parse::<u32>()
                    .unwrap_or(0);
                n.signal = n.signal.max(pct);
            }
        }
    }
    if let Some(n) = cur.take() {
        nets.push(n);
    }
    nets.retain(|n| !n.ssid.is_empty());
    nets.sort_by(|a, b| {
        let ac = connected.as_deref() == Some(a.ssid.as_str());
        let bc = connected.as_deref() == Some(b.ssid.as_str());
        bc.cmp(&ac).then(b.signal.cmp(&a.signal))
    });
    nets.dedup_by(|a, b| a.ssid == b.ssid);
    nets.truncate(6);
    (connected, nets)
}

fn wifi_post(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_POP, WPARAM(0), LPARAM(0));
    }
}

/// Refresh the connected network and the list of visible ones (only when the
/// dropdown opens).
pub fn wifi_scan_async(hwnd: isize) {
    std::thread::spawn(move || {
        let (connected, nets) = wifi_scan_blocking();
        let mut w = WIFI.lock().unwrap();
        w.connected = connected;
        w.nets = nets;
        drop(w);
        wifi_post(hwnd);
    });
}

/// SSID we are fully connected to (None while associating or disconnected).
fn wifi_connected_ssid() -> Option<String> {
    let out = netsh("wlan show interfaces");
    let mut ssid = None;
    let mut up = false;
    for line in out.lines() {
        let l = line.trim();
        if l.starts_with("SSID") && !l.starts_with("BSSID") && ssid.is_none() {
            ssid = value_after_colon(l).filter(|v| !v.is_empty()).map(|v| v.to_string());
        } else if l.starts_with("State") {
            up = value_after_colon(l)
                .map(|v| v.eq_ignore_ascii_case("connected"))
                .unwrap_or(false);
        }
    }
    if up {
        ssid
    } else {
        None
    }
}

/// Wait up to ~10 s for the connection to come up; returns true if it did.
fn wifi_wait_connected(ssid: &str) -> bool {
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        if wifi_connected_ssid().as_deref() == Some(ssid) {
            return true;
        }
    }
    false
}

fn wifi_finish(hwnd: isize, text: String, ok: bool) {
    let (connected, nets) = wifi_scan_blocking();
    let mut w = WIFI.lock().unwrap();
    w.connected = connected;
    w.nets = nets;
    w.busy = None;
    w.last = Some((text, ok));
    drop(w);
    wifi_post(hwnd);
}

/// Only the connected-network name (cheap; used for the hover tooltip).
pub fn wifi_status_async(hwnd: isize) {
    std::thread::spawn(move || {
        let c = wifi_connected_ssid();
        WIFI.lock().unwrap().connected = c;
        wifi_post(hwnd);
    });
}

/// Join a saved network. `None` disconnects.
pub fn wifi_connect_async(hwnd: isize, ssid: Option<String>) {
    WIFI.lock().unwrap().busy = ssid.clone();
    wifi_post(hwnd);
    std::thread::spawn(move || match ssid {
        Some(s) => {
            netsh(&format!("wlan connect name=\"{s}\" ssid=\"{s}\""));
            let ok = wifi_wait_connected(&s);
            let text = if ok {
                format!("Connected to {s}")
            } else {
                format!("Couldn't connect to {s}")
            };
            wifi_finish(hwnd, text, ok);
        }
        None => {
            netsh("wlan disconnect");
            std::thread::sleep(std::time::Duration::from_millis(1200));
            wifi_finish(hwnd, "Disconnected".into(), true);
        }
    });
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Is this a network type we can join with just a password?
pub fn wifi_auth_supported(auth: &str) -> bool {
    let a = auth.to_ascii_lowercase();
    a == "open" || a == "wpa2-personal" || a == "wpa3-personal" || a == "wpa-personal"
}

fn wifi_profile_xml(ssid: &str, auth: &str, key: Option<&str>) -> String {
    let a = auth.to_ascii_lowercase();
    let (authn, enc) = match (key.is_some(), a.as_str()) {
        (false, _) => ("open", "none"),
        (true, "wpa3-personal") => ("WPA3SAE", "AES"),
        (true, "wpa-personal") => ("WPAPSK", "TKIP"),
        _ => ("WPA2PSK", "AES"),
    };
    let ssid = xml_escape(ssid);
    let shared = match key {
        Some(k) => format!(
            "<sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>",
            xml_escape(k)
        ),
        None => String::new(),
    };
    format!(
        "<?xml version=\"1.0\"?><WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\">\
<name>{ssid}</name><SSIDConfig><SSID><name>{ssid}</name></SSID></SSIDConfig>\
<connectionType>ESS</connectionType><connectionMode>auto</connectionMode>\
<MSM><security><authEncryption><authentication>{authn}</authentication><encryption>{enc}</encryption><useOneX>false</useOneX></authEncryption>{shared}</security></MSM></WLANProfile>"
    )
}

/// Join a network that has no saved profile (open, or with a password the user
/// just typed). The profile is kept only if the connection works; the temporary
/// file holding the password is deleted straight away.
pub fn wifi_join_new_async(hwnd: isize, ssid: String, auth: String, key: Option<String>) {
    WIFI.lock().unwrap().busy = Some(ssid.clone());
    wifi_post(hwnd);
    std::thread::spawn(move || {
        let xml = wifi_profile_xml(&ssid, &auth, key.as_deref());
        let path = std::env::temp_dir().join(format!("nock-wlan-{}.xml", std::process::id()));
        let wrote = std::fs::write(&path, xml).is_ok();
        let mut ok = false;
        if wrote {
            netsh(&format!("wlan add profile filename=\"{}\" user=current", path.display()));
            let _ = std::fs::remove_file(&path);
            netsh(&format!("wlan connect name=\"{ssid}\" ssid=\"{ssid}\""));
            ok = wifi_wait_connected(&ssid);
            if !ok {
                // do not keep a profile with a wrong password
                netsh(&format!("wlan delete profile name=\"{ssid}\""));
            }
        }
        let text = if ok {
            format!("Connected to {ssid}")
        } else if key.is_some() {
            format!("Couldn't join {ssid}. Check the password")
        } else {
            format!("Couldn't connect to {ssid}")
        };
        wifi_finish(hwnd, text, ok);
    });
}

/// Friendly name of the app owning the foreground window (None if it is one of `skip`).
pub fn foreground_app_name(skip: &[HWND]) -> Option<String> {
    use windows::Win32::System::Threading::*;
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() || skip.contains(&fg) {
            return None;
        }
        // With nothing open the foreground is the desktop or taskbar, which belong
        // to explorer.exe but are not a File Explorer window.
        let mut cls = [0u16; 64];
        let n = GetClassNameW(fg, &mut cls) as usize;
        match String::from_utf16_lossy(&cls[..n.min(64)]).as_str() {
            "Progman" | "WorkerW" => return Some("Desktop".to_string()),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "NotifyIconOverflowWindow" => return None,
            _ => {}
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let stem = Path::new(&path).file_stem()?.to_string_lossy().to_string();
        Some(friendly(&stem))
    }
}

fn friendly(stem: &str) -> String {
    match stem.to_ascii_lowercase().as_str() {
        "msedge" => "Microsoft Edge".into(),
        "chrome" => "Google Chrome".into(),
        "firefox" => "Firefox".into(),
        "code" => "VS Code".into(),
        "explorer" => "File Explorer".into(),
        "windowsterminal" => "Terminal".into(),
        "applicationframehost" => "App".into(),
        "searchhost" | "startmenuexperiencehost" => "Windows".into(),
        "spotify" => "Spotify".into(),
        "whatsapp" | "whatsapp.root" => "WhatsApp".into(),
        _ => {
            let mut c = stem.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

// ------------------------------------------------------------ reserved strip

/// Reserve (or release) a strip of `height_px` at the top of the monitor so
/// maximised windows stop below the notch. Pass `height_px = 0` to release.
/// All displays, primary first, then left to right.
pub fn monitors() -> Vec<HMONITOR> {
    unsafe extern "system" fn cb(m: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        unsafe { (*(lp.0 as *mut Vec<HMONITOR>)).push(m) };
        TRUE
    }
    let mut v: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut v as *mut _ as isize));
    }
    let info = |m: HMONITOR| unsafe {
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        (mi.dwFlags & 1 == 0, mi.rcMonitor.left, mi.rcMonitor.top) // MONITORINFOF_PRIMARY = 1
    };
    v.sort_by_key(|m| info(*m));
    v
}

/// The display chosen in settings (0 = primary); falls back to primary if it is unplugged.
pub fn pick_monitor(idx: u8) -> HMONITOR {
    let list = monitors();
    match list.get((idx as usize).saturating_sub(1)) {
        Some(m) if idx > 0 => *m,
        _ => unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) },
    }
}

pub fn scale_of(mon: HMONITOR) -> f32 {
    let (mut dx, mut dy) = (0u32, 0u32);
    unsafe {
        if windows::Win32::UI::HiDpi::GetDpiForMonitor(mon, windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() && dx > 0 {
            return dx as f32 / 96.0;
        }
    }
    1.0
}

pub fn reserve_top(hwnd: HWND, mon: &RECT, height_px: i32) {
    const ABM_NEW: u32 = 0;
    const ABM_REMOVE: u32 = 1;
    const ABM_SETPOS: u32 = 3;
    const ABE_TOP: u32 = 1;
    thread_local! {
        static REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            ..Default::default()
        };
        let reg = REGISTERED.with(|r| r.get());
        if height_px <= 0 {
            if reg {
                SHAppBarMessage(ABM_REMOVE, &mut abd);
                REGISTERED.with(|r| r.set(false));
            }
            return;
        }
        if !reg {
            SHAppBarMessage(ABM_NEW, &mut abd);
            REGISTERED.with(|r| r.set(true));
        }
        abd.uEdge = ABE_TOP;
        abd.rc = RECT {
            left: mon.left,
            top: mon.top,
            right: mon.right,
            bottom: mon.top + height_px,
        };
        SHAppBarMessage(ABM_SETPOS, &mut abd);
    }
}

// ---------------------------------------------------------------- launching

pub fn open(target: &str, args: &str) {
    unsafe {
        let t = HSTRING::from(target);
        let a = HSTRING::from(args);
        let params = if args.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(a.as_ptr())
        };
        ShellExecuteW(None, w!("open"), &t, params, PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Pick a local CSV or TSV file without adding a heavyweight dialog dependency.
pub fn pick_data_file() -> Option<PathBuf> {
use windows::Win32::UI::Controls::Dialogs::{GetOpenFileNameW, OPENFILENAMEW, OFN_FILEMUSTEXIST};
    let mut path = [0u16; 32_768];
    let filter: Vec<u16> = "CSV and TSV files\0*.csv;*.tsv;*.tab\0All files\0*.*\0\0".encode_utf16().collect();
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND::default(),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(path.as_mut_ptr()),
        nMaxFile: path.len() as u32,
        Flags: OFN_FILEMUSTEXIST,
        ..Default::default()
    };
    unsafe {
        if !GetOpenFileNameW(&mut dialog).as_bool() { return None; }
    }
    let end = path.iter().position(|c| *c == 0).unwrap_or(path.len());
    Some(PathBuf::from(String::from_utf16_lossy(&path[..end])))
}

pub fn lock_pc() {
    unsafe {
        let _ = LockWorkStation();
    }
}

pub fn sleep_pc() {
    open("rundll32.exe", "powrprof.dll,SetSuspendState 0,1,0");
}

// ---------------------------------------------------------------- clipboard

/// Password managers mark their copies with this format; we must not record them.
pub fn clipboard_excluded() -> bool {
    unsafe {
        let f = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        f != 0 && IsClipboardFormatAvailable(f).is_ok()
    }
}

pub fn clipboard_text(hwnd: HWND) -> Option<String> {
    unsafe {
        if OpenClipboard(Some(hwnd)).is_err() {
            return None;
        }
        let result = (|| {
            let h = GetClipboardData(CF_UNICODETEXT).ok()?;
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u16;
            if p.is_null() {
                return None;
            }
            let mut len = 0usize;
            while *p.add(len) != 0 && len < 20_000 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            let _ = GlobalUnlock(g);
            Some(s)
        })();
        let _ = CloseClipboard();
        result
    }
}

pub fn set_clipboard_text(hwnd: HWND, s: &str) -> bool {
    unsafe {
        let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2) else {
            return false;
        };
        let p = GlobalLock(hmem) as *mut u16;
        if p.is_null() {
            let _ = GlobalFree(Some(hmem));
            return false;
        }
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        let _ = GlobalUnlock(hmem);
        if OpenClipboard(Some(hwnd)).is_err() {
            let _ = GlobalFree(Some(hmem));
            return false;
        }
        let _ = EmptyClipboard();
        let ok = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hmem.0))).is_ok();
        let _ = CloseClipboard();
        if !ok {
            let _ = GlobalFree(Some(hmem));
        }
        ok
    }
}

pub fn send_ctrl_v() {
    unsafe {
        let key = |vk: u16, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [
            key(0x11, false),
            key(0x56, false),
            key(0x56, true),
            key(0x11, true),
        ];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

// ---------------------------------------------------------------- autostart

/// Read the actual Run command, not just whether a stale entry exists.
fn autostart_command() -> Option<String> {
    unsafe {
        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let mut bytes = 0u32;
        if RegGetValueW(HKEY_CURRENT_USER, key, w!("Nock"), RRF_RT_REG_SZ, None, None, Some(&mut bytes)) != ERROR_SUCCESS || bytes < 2 {
            return None;
        }
        let mut wide = vec![0u16; (bytes as usize + 1) / 2];
        if RegGetValueW(HKEY_CURRENT_USER, key, w!("Nock"), RRF_RT_REG_SZ, None, Some(wide.as_mut_ptr().cast()), Some(&mut bytes)) != ERROR_SUCCESS {
            return None;
        }
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        Some(String::from_utf16_lossy(&wide[..end]))
    }
}

pub fn autostart_enabled() -> bool {
    autostart_command().is_some()
}

fn expected_autostart_command(exe: &std::path::Path) -> String {
    expected_autostart_command_with_family(exe, package_family_name().as_deref())
}

fn expected_autostart_command_with_family(exe: &std::path::Path, family: Option<&str>) -> String {
    // The versioned WindowsApps directory changes on every MSIX update.
    // Start via the package's stable application ID instead of that path.
    let path = exe.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
        if path.contains("\\windowsapps\\") {
        if let Some(family) = family {
            return format!("cmd.exe /c start \"\" \"shell:AppsFolder\\{family}!Nock\"");
        }
        format!("\"{}\"", exe.display())
    } else {
        format!("\"{}\"", exe.display())
    }
}

fn package_family_name() -> Option<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentPackageFamilyName(length: *mut u32, family_name: *mut u16) -> u32;
    }
    unsafe {
        let mut length = 0u32;
        let status = GetCurrentPackageFamilyName(&mut length, std::ptr::null_mut());
        if status != 122 || length == 0 { return None; }
        let mut name = vec![0u16; length as usize];
        if GetCurrentPackageFamilyName(&mut length, name.as_mut_ptr()) != 0 { return None; }
        let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
        Some(String::from_utf16_lossy(&name[..end]))
    }
}

#[cfg(test)]
mod autostart_tests {
    #[test]
    fn packaged_updates_keep_a_stable_startup_target() {
        let old = std::path::Path::new(r"C:\Program Files\WindowsApps\Notchix.Desktop_0.13.8.22_x64__package\nock.exe");
        let new = std::path::Path::new(r"C:\Program Files\WindowsApps\Notchix.Desktop_0.13.8.23_x64__package\nock.exe");
        let family = "Notchix.Desktop_abc123";
        let old_command = super::expected_autostart_command_with_family(old, Some(family));
        let new_command = super::expected_autostart_command_with_family(new, Some(family));
        assert_eq!(old_command, new_command);
        assert!(new_command.contains("shell:AppsFolder\\Notchix.Desktop_abc123!Nock"));
    }
}

/// When the user extracts an updated ZIP to a new location, Windows may still
/// point at the old executable. Preserve the opt-in setting but repair its path.
pub fn repair_autostart_path() {
    if let (Some(saved), Ok(exe)) = (autostart_command(), std::env::current_exe()) {
        let expected = expected_autostart_command(&exe);
        if !saved.eq_ignore_ascii_case(&expected) {
            set_autostart(true);
        }
    }
}

pub fn set_autostart(on: bool) {
    unsafe {
        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        if on {
            if let Ok(exe) = std::env::current_exe() {
                let cmd = expected_autostart_command(&exe);
                let w: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key,
                    w!("Nock"),
                    REG_SZ.0,
                    Some(w.as_ptr() as *const _),
                    (w.len() * 2) as u32,
                );
            }
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, key, w!("Nock"));
        }
    }
}

// ------------------------------------------------------------------- state

/// True when a window is really full screen: it matches the monitor rectangle exactly
/// AND has no title bar. A maximised normal window also matches the monitor exactly when
/// the taskbar is set to auto-hide, so the title bar is what tells the two apart (browsers,
/// video players and games drop it when they go full screen).
pub(crate) fn is_fullscreen_window(r: &RECT, mon: &RECT, style: u32) -> bool {
    const WS_CAPTION_BITS: u32 = 0x00C0_0000; // WS_BORDER | WS_DLGFRAME
    // Borderless games sometimes report a small invisible resize border or
    // change resolution during launch. Exact rectangle equality misses them.
    let covers = (r.left - mon.left).abs() <= 16
        && (r.top - mon.top).abs() <= 16
        && (r.right - mon.right).abs() <= 16
        && (r.bottom - mon.bottom).abs() <= 16;
    covers && style & WS_CAPTION_BITS != WS_CAPTION_BITS
}

/// Window class name of `h` (for log lines).
pub fn class_of(h: HWND) -> String {
    unsafe {
        let mut cls = [0u16; 64];
        let n = GetClassNameW(h, &mut cls) as usize;
        String::from_utf16_lossy(&cls[..n.min(64)])
    }
}

/// Windows' own overlay windows (Start, Search, Task View, the Alt-Tab switcher, flyouts).
/// They can cover the whole monitor, and some linger as the foreground window after a
/// flyout closes, but they are not full-screen apps and must not hide Nock.
pub(crate) fn is_shell_overlay(class: &str) -> bool {
    matches!(
        class,
        "Windows.UI.Core.CoreWindow"
            | "XamlExplorerHostIslandWindow"
            | "MultitaskingViewFrame"
            | "ForegroundStaging"
            | "Windows.UI.Input.InputSite.WindowClass"
            | "NativeHWNDHost"
            | "TopLevelWindowForOverflowXamlIsland"
    )
}

static LAST_KIND: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn log_visibility(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::store::data_dir().join("visibility.log")) {
        let _ = writeln!(f, "{msg}");
    }
}

/// 0 = none, 1 = a full-screen video/browser/app (the bar can be peeked at),
/// 2 = a game or presentation (stay out of the way completely).
/// `skip` are our own windows; `mon` is the primary monitor rectangle.
///
/// The shell's "user notification state" is only trusted when the foreground window
/// itself is really full screen: on some PCs it reports "busy" all the time, which used
/// to keep the notch hidden even with nothing full screen.
pub fn fullscreen_kind(skip: &[HWND], mon: &RECT) -> u8 {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() || skip.contains(&fg) {
            return 0;
        }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(fg, &mut cls) as usize;
        let cls = String::from_utf16_lossy(&cls[..n.min(64)]);
        if matches!(cls.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return 0;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return 0;
        }
        let style = GetWindowLongPtrW(fg, GWL_STYLE) as u32;
        let app = foreground_app_name(skip).unwrap_or_default().to_ascii_lowercase();
        let game = app.contains("league of legends") || app.contains("valorant");
        let covers_game_screen = (r.left - mon.left).abs() <= 24
            && (r.top - mon.top).abs() <= 24
            && (r.right - mon.right).abs() <= 24
            && (r.bottom - mon.bottom).abs() <= 24;
        let kind = if is_fullscreen_window(&r, mon, style) || (game && covers_game_screen) {
            // Windows itself says a game or presentation owns the screen: trust that.
            // Otherwise a shell overlay (Start, Search, Task View...) is not full screen.
            let hard = matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE);
            if hard || game {
                2
            } else if is_shell_overlay(&cls) {
                0
            } else {
                1
            }
        } else {
            0
        };
        // leave a trace when the verdict changes, so "it hides for no reason" can be diagnosed
        if LAST_KIND.swap(kind, std::sync::atomic::Ordering::Relaxed) != kind && kind != 0 {
            log_visibility(&format!(
                "hiding (kind {kind}): class '{cls}', rect {},{},{},{}, style {style:#x}, monitor {},{},{},{}",
                r.left, r.top, r.right, r.bottom, mon.left, mon.top, mon.right, mon.bottom
            ));
        }
        kind
    }
}

#[cfg(test)]
mod fullscreen_tests {
    use super::*;

    fn rc(l: i32, t: i32, r: i32, b: i32) -> RECT {
        RECT { left: l, top: t, right: r, bottom: b }
    }

    #[test]
    fn maximised_window_is_not_full_screen() {
        let mon = rc(0, 0, 1920, 1080);
        // maximised with an auto-hidden taskbar: exact size, but it has a title bar
        assert!(!is_fullscreen_window(&rc(0, 0, 1920, 1080), &mon, 0x16CF_0000));
        // maximised normally: overhangs the monitor
        assert!(!is_fullscreen_window(&rc(-8, -8, 1928, 1040), &mon, 0x16CF_0000));
    }

    #[test]
    fn borderless_game_with_resize_overhang_counts_as_full_screen() {
        let mon = rc(0, 0, 1920, 1080);
        assert!(is_fullscreen_window(&rc(-8, -8, 1928, 1088), &mon, 0x9400_0000));
        assert!(!is_fullscreen_window(&rc(0, 0, 1700, 980), &mon, 0x9400_0000));
    }

    #[test]
    fn windows_overlays_do_not_count_as_full_screen_apps() {
        assert!(is_shell_overlay("Windows.UI.Core.CoreWindow"));
        assert!(is_shell_overlay("MultitaskingViewFrame"));
        assert!(!is_shell_overlay("Chrome_WidgetWin_1"));
        assert!(!is_shell_overlay("ApplicationFrameWindow"));
    }

    #[test]
    fn borderless_full_screen_is_detected() {
        let mon = rc(0, 0, 1920, 1080);
        assert!(is_fullscreen_window(&rc(0, 0, 1920, 1080), &mon, 0x9400_0000));
        assert!(!is_fullscreen_window(&rc(0, 0, 1900, 1080), &mon, 0x9400_0000));
    }
}

// ------------------------------------------- full-screen change notifications
// Entering full screen inside the same window (YouTube, F11) does not change the
// foreground window, so also listen for the foreground window moving or resizing.

/// Posted when the foreground window changed size or position.
pub const WM_FSCHK: u32 = 0x8000 + 7; // WM_APP + 7

static LOC_HOOK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static LOC_NOTIFY: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

unsafe extern "system" fn loc_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    use std::sync::atomic::Ordering::Relaxed;
    if object == 0 && hwnd == GetForegroundWindow() {
        let _ = PostMessageW(
            Some(HWND(LOC_NOTIFY.load(Relaxed) as *mut _)),
            WM_FSCHK,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

/// (Re)attach the location listener to the process that owns the foreground window.
pub fn watch_foreground_location(notify: HWND) {
    use std::sync::atomic::Ordering::Relaxed;
    use windows::Win32::UI::Accessibility::*;
    unsafe {
        LOC_NOTIFY.store(notify.0 as isize, Relaxed);
        let old = LOC_HOOK.swap(0, Relaxed);
        if old != 0 {
            let _ = UnhookWinEvent(HWINEVENTHOOK(old as *mut _));
        }
        let fg = GetForegroundWindow();
        if fg.is_invalid() || fg == notify {
            return;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == 0 {
            return;
        }
        let h = SetWinEventHook(
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(loc_proc),
            pid,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        LOC_HOOK.store(h.0 as isize, Relaxed);
    }
}

// ------------------------------------------------------- top-edge detector
// While a full-screen app is running we watch for the pointer touching the
// top edge (centre zone) so the bar can slide in. The hook exists only during
// full screen; its callback does a few integer compares and nothing else.

/// Posted once when the pointer enters the top-edge zone.
pub const WM_EDGE: u32 = 0x8000 + 6; // WM_APP + 6

static EDGE_HOOK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static EDGE_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static EDGE_L: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static EDGE_R: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static EDGE_Y: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static EDGE_IN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

unsafe extern "system" fn edge_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    use std::sync::atomic::Ordering::Relaxed;
    if code >= 0 && wp.0 as u32 == WM_MOUSEMOVE {
        let ms = &*(lp.0 as *const MSLLHOOKSTRUCT);
        let inside = ms.pt.y <= EDGE_Y.load(Relaxed)
            && ms.pt.x >= EDGE_L.load(Relaxed)
            && ms.pt.x < EDGE_R.load(Relaxed);
        if inside != EDGE_IN.swap(inside, Relaxed) && inside {
            let _ = PostMessageW(
                Some(HWND(EDGE_HWND.load(Relaxed) as *mut _)),
                WM_EDGE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

/// Start or stop watching the top edge. `zone` = (left, right, y) in screen pixels.
pub fn edge_watch(on: bool, hwnd: HWND, zone: (i32, i32, i32)) {
    use std::sync::atomic::Ordering::Relaxed;
    unsafe {
        EDGE_L.store(zone.0, Relaxed);
        EDGE_R.store(zone.1, Relaxed);
        EDGE_Y.store(zone.2, Relaxed);
        EDGE_HWND.store(hwnd.0 as isize, Relaxed);
        let cur = EDGE_HOOK.load(Relaxed);
        if on && cur == 0 {
            EDGE_IN.store(false, Relaxed);
            let hinst = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .map(|m| HINSTANCE(m.0))
                .ok();
            if let Ok(h) = SetWindowsHookExW(WH_MOUSE_LL, Some(edge_proc), hinst, 0) {
                EDGE_HOOK.store(h.0 as isize, Relaxed);
            }
        } else if !on && cur != 0 {
            let _ = UnhookWindowsHookEx(HHOOK(cur as *mut _));
            EDGE_HOOK.store(0, Relaxed);
        }
    }
}

/// (year, month 1-12, day, weekday 0=Sunday, hour, minute, second, millisecond)
pub fn local_time() -> (i32, u32, u32, u32, u32, u32, u32, u32) {
    unsafe {
        let t = GetLocalTime();
        (
            t.wYear as i32,
            t.wMonth as u32,
            t.wDay as u32,
            t.wDayOfWeek as u32,
            t.wHour as u32,
            t.wMinute as u32,
            t.wSecond as u32,
            t.wMilliseconds as u32,
        )
    }
}

pub fn filetime_now() -> i64 {
    unsafe {
        let ft = GetSystemTimePreciseAsFileTime();
        ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
    }
}

// -------------------------------------------------------------------- icons

/// Resolve "calc.exe" / "C:\\...\\app.exe" to a real path so we can fetch its icon.
pub fn resolve_path(target: &str) -> Option<PathBuf> {
    let p = Path::new(target);
    if p.is_absolute() {
        return p.exists().then(|| p.to_path_buf());
    }
    if target.contains("://") || target.ends_with(':') || target.contains(':') {
        return None; // URL or ms-settings: etc.
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(root) = std::env::var_os("SystemRoot") {
        let root = PathBuf::from(root);
        dirs.push(root.join("System32"));
        dirs.push(root);
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter()
        .map(|d| d.join(target))
        .find(|c| c.exists())
}

pub fn file_icon(path: &Path) -> Option<HICON> {
    unsafe {
        let w = HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
        let mut sfi = SHFILEINFOW::default();
        let r = SHGetFileInfoW(
            &w,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut sfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if r == 0 || sfi.hIcon.is_invalid() {
            None
        } else {
            Some(sfi.hIcon)
        }
    }
}

/// Idle and total CPU time (100 ns ticks) since boot, for load between two reads.
pub fn cpu_times() -> (u64, u64) {
    use windows::Win32::System::Threading::GetSystemTimes;
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    let q = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    unsafe {
        if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_err() {
            return (0, 0);
        }
    }
    // kernel time already includes idle time
    (q(idle), q(kernel) + q(user))
}

/// Percent of physical memory in use.
pub fn mem_percent() -> u32 {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe {
        let _ = GlobalMemoryStatusEx(&mut m);
    }
    m.dwMemoryLoad
}

thread_local! {
    static METER: RefCell<Option<(IAudioMeterInformation, std::time::Instant)>> = const { RefCell::new(None) };
}

/// Current output peak level 0..1 (what the speakers are playing), if available.
pub fn audio_peak() -> Option<f32> {
    METER.with(|m| {
        // re-bind every few seconds so a changed default output device is followed
        let stale = m.borrow().as_ref().map(|(_, t)| t.elapsed().as_secs() >= 3).unwrap_or(true);
        if stale {
            let got = unsafe {
                let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
                let dev = en.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
                dev.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None).ok()
            };
            *m.borrow_mut() = got.map(|g| (g, std::time::Instant::now()));
        }
        let meter = m.borrow().as_ref()?.0.clone();
        match unsafe { meter.GetPeakValue() } {
            Ok(v) => Some(v.clamp(0.0, 1.0)),
            Err(_) => {
                *m.borrow_mut() = None; // default device probably changed
                None
            }
        }
    })
}

fn shutdown_cmd(arg: &str) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("shutdown.exe").args([arg, "/t", "0"]).creation_flags(0x0800_0000).spawn();
}

pub fn restart_pc() {
    shutdown_cmd("/r");
}

pub fn shutdown_pc() {
    shutdown_cmd("/s");
}

/// Seconds since Windows started.
pub fn uptime_secs() -> u64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() / 1000 }
}

pub const WM_APPLIST: u32 = 0x8000 + 9; // WM_APP + 9 (8 is radio::WM_BTDONE)

/// Installed apps for the picker: (name, app id). Filled by `list_start_apps`.
pub static START_APPS: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Ask Windows for every app in the Start menu (classic, Store and game launchers) in
/// the background, then post WM_APPLIST. Takes about a second.
pub fn list_start_apps(hwnd: isize) {
    std::thread::spawn(move || {
        let out = powershell("[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-StartApps | ForEach-Object { $_.Name + '|' + $_.AppID }").unwrap_or_default();
        let mut v: Vec<(String, String)> = out
            .lines()
            .filter_map(|l| {
                let (n, id) = l.split_once('|')?;
                let (n, id) = (n.trim(), id.trim());
                (!n.is_empty() && !id.is_empty()).then(|| (n.to_string(), id.to_string()))
            })
            .collect();
        // hide the entries nobody wants to pin
        let noisy = ["uninstall", "verifier", "readme", "release notes", "documentation", "eula"];
        v.retain(|(n, _)| {
            let l = n.to_lowercase();
            !noisy.iter().any(|x| l.contains(x))
        });
        v.sort_by_key(|(n, _)| n.to_lowercase());
        v.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
        *START_APPS.lock().unwrap() = v;
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_APPLIST, WPARAM(0), LPARAM(0));
        }
    });
}

/// Icon of a Start-menu app (works for Store apps too) from its `shell:AppsFolder\...` name.
pub fn shell_icon(parse_name: &str) -> Option<HICON> {
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{SHParseDisplayName, SHGFI_PIDL};
    unsafe {
        let name = HSTRING::from(parse_name);
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        SHParseDisplayName(&name, None::<&windows::Win32::System::Com::IBindCtx>, &mut pidl, 0, None).ok()?;
        let mut sfi = SHFILEINFOW::default();
        let r = SHGetFileInfoW(
            PCWSTR(pidl as *const u16),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut sfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_PIDL | SHGFI_ICON | SHGFI_LARGEICON,
        );
        windows::Win32::System::Com::CoTaskMemFree(Some(pidl as *const _));
        if r == 0 || sfi.hIcon.is_invalid() {
            None
        } else {
            Some(sfi.hIcon)
        }
    }
}

/// Where the Start Menu shortcut lives (per user, so no admin rights are needed).
fn start_menu_link() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(std::path::PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs\Notchix.lnk"))
}

/// Add (or remove) a Start Menu shortcut so typing "Nock" in Windows search finds it. A
/// plain .exe is invisible to search; a shortcut in this folder is what makes an app
/// "installed". Rewritten at every start so it follows the exe if it is moved.
pub fn sync_start_menu(on: bool) {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    let Some(lnk) = start_menu_link() else { return };
    if !on {
        let _ = std::fs::remove_file(&lnk);
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    unsafe {
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { return };
        let _ = link.SetPath(&HSTRING::from(exe.as_os_str().to_string_lossy().as_ref()));
        if let Some(dir) = exe.parent() {
            let _ = link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str().to_string_lossy().as_ref()));
        }
        let _ = link.SetDescription(w!("Notchix - a lightweight desktop island for Windows"));
        let _ = link.SetIconLocation(&HSTRING::from(exe.as_os_str().to_string_lossy().as_ref()), 0);
        if let Ok(file) = link.cast::<IPersistFile>() {
            let _ = file.Save(&HSTRING::from(lnk.as_os_str().to_string_lossy().as_ref()), true);
        }
    }
}

#[cfg(test)]
mod start_menu_tests {
    #[test]
    #[ignore]
    fn creates_and_removes_the_shortcut() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        }
        super::sync_start_menu(true);
        let lnk = super::start_menu_link().unwrap();
        let made = lnk.exists();
        println!("PROBE created={made} path={}", lnk.display());
        std::thread::sleep(std::time::Duration::from_secs(2));
        super::sync_start_menu(false);
        println!("PROBE removed={}", !lnk.exists());
    }
}

// ------------------------------------------------------------------ updates ----

pub const WM_UPDATE: u32 = 0x8000 + 10; // WM_APP + 10
const REPO: &str = "RoeyRaz/Notchix";

/// Newest release tag found by `check_update` (e.g. "v0.9.1").
pub static LATEST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// True if `latest` ("v1.2.3" or "1.2.3") is a higher version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let nums = |s: &str| -> Vec<u32> { s.trim().trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    nums(latest) > nums(current)
}

/// Ask GitHub for the latest release in the background, then post WM_UPDATE:
/// WPARAM 0 = could not reach GitHub, 1 = up to date, 2 = a newer version exists.
pub fn check_update(hwnd: isize) {
    std::thread::spawn(move || {
        let cmd = format!(
            "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; (Invoke-RestMethod -UseBasicParsing -Headers @{{'User-Agent'='Nock'}} -Uri 'https://api.github.com/repos/{REPO}/releases/latest').tag_name"
        );
        let tag = powershell(&cmd).unwrap_or_default().trim().to_string();
        let code = if !tag.starts_with('v') {
            0
        } else if is_newer(&tag, env!("CARGO_PKG_VERSION")) {
            2
        } else {
            1
        };
        if code != 0 {
            *LATEST.lock().unwrap() = tag;
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_UPDATE, WPARAM(code), LPARAM(0));
        }
    });
}

/// Whether this exe is the one the installer put in place (so the installer can replace it).
pub fn installed_by_setup() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    // the installer's uninstaller sits beside an installed copy, whatever spelling the path has
    exe.parent().is_some_and(|d| d.join("unins000.exe").exists())
        || std::env::var("LOCALAPPDATA").is_ok_and(|l| is_install_path(&exe.to_string_lossy(), &l))
}

/// Is `exe` inside a per-user product install folder? Ignores case and slash style.
fn is_install_path(exe: &str, local: &str) -> bool {
    let norm = |s: &str| s.trim_start_matches(r"\\?\").replace('/', "\\").to_lowercase();
    let base = format!("{}\\programs\\", norm(local).trim_end_matches('\\'));
    norm(exe).strip_prefix(&base).is_some_and(|rest| {
        rest.starts_with("notchix\\") || rest.starts_with("nock\\")
    })
}

/// Download the Notchix installer for `tag`, run it silently, then restart the app.
pub fn install_update(tag: &str) {
    let tag = tag.to_string();
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default().replace('\'', "''");
    std::thread::spawn(move || {
        let ver = tag.trim_start_matches('v');
        let cmd = format!(
            "$ErrorActionPreference='Stop'; try {{ [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
             $f = Join-Path $env:TEMP 'Notchix-Setup-update.exe'; \
             Invoke-WebRequest -UseBasicParsing -Uri 'https://github.com/{REPO}/releases/download/{tag}/Notchix-Setup-{ver}.exe' -OutFile $f; \
             Start-Process -FilePath $f -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -Wait }} catch {{}}; \
             Start-Process -FilePath '{exe}'"
        );
        let _ = powershell(&cmd);
    });
}

/// Update a copy that was not installed by the installer (the portable exe): download
/// `Notchix-portable-<ver>.exe`, and only once that worked stop this app, put the new file
/// over this exe and start it. If the file cannot be replaced (a read-only folder) the
/// downloads page opens instead. A failed download leaves Nock running untouched.
pub fn update_portable(tag: &str) {
    let tag = tag.to_string();
    let pid = std::process::id();
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default().replace('\'', "''");
    std::thread::spawn(move || {
        let ver = tag.trim_start_matches('v');
        let cmd = format!(
            "$ErrorActionPreference='Stop'; try {{ [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
             $f = Join-Path $env:TEMP 'Notchix-portable-update.exe'; \
             Invoke-WebRequest -UseBasicParsing -Uri 'https://github.com/{REPO}/releases/download/{tag}/Notchix-portable-{ver}.exe' -OutFile $f; \
             if ((Get-Item $f).Length -lt 100000) {{ throw 'bad download' }} }} catch {{ exit 1 }}; \
             Stop-Process -Id {pid} -Force; Start-Sleep -Seconds 2; \
             try {{ Copy-Item -Force $f '{exe}' }} catch {{ Start-Process 'https://github.com/{REPO}/releases/latest' }}; \
             Start-Process -FilePath '{exe}'"
        );
        let _ = powershell(&cmd);
    });
}

/// Open the releases page (for copies that were not installed by the installer).
pub fn open_releases() {
    open(&format!("https://github.com/{REPO}/releases/latest"), "");
}

#[cfg(test)]
mod update_tests {
    use super::{is_install_path, is_newer};

    #[test]
    fn install_path_detection() {
        let local = r"C:\Users\User\AppData\Local";
        assert!(is_install_path(r"C:\Users\User\AppData\Local\Programs\Notchix\nock.exe", local));
        assert!(is_install_path(r"c:\users\USER\appdata\local\programs\nock\NOCK.EXE", local));
        assert!(is_install_path(r"\\?\C:\Users\User\AppData\Local\Programs\Notchix\nock.exe", local));
        assert!(!is_install_path(r"C:\Users\User\AppData\Local\Programs\NotchixOther\nock.exe", local));
        assert!(!is_install_path(r"V:\Projects\nock\target\release\nock.exe", local));
    }

    #[test]
    fn version_compare() {
        assert!(is_newer("v0.9.1", "0.9.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("v0.9.0", "0.9.0"));
        assert!(!is_newer("v0.8.1", "0.9.0"));
    }
}
