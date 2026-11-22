//! Read genuine Windows Explorer file-operation progress from its copy dialog.
//! No filenames or destinations are read, and no percentage is inferred from I/O.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    SendMessageTimeoutW, SMTO_ABORTIFHUNG,
};

static PROGRESS: OnceLock<Mutex<Option<(f32, Instant)>>> = OnceLock::new();
static BUSY: AtomicBool = AtomicBool::new(false);
static CHANGED: AtomicBool = AtomicBool::new(false);

pub fn current() -> Option<f32> {
    let guard = PROGRESS.get()?.lock().ok()?;
    let (progress, seen) = guard.as_ref()?;
    (seen.elapsed() < Duration::from_secs(5)).then_some(*progress)
}

pub fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::AcqRel)
}

/// Called on the existing two-second transfer timer. The scan itself runs off
/// the UI thread and does not add a persistent background worker.
pub fn sample_async() {
    if BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    if std::thread::Builder::new()
        .name("explorer-copy".into())
        .spawn(|| {
            let found = scan();
            if let Ok(mut guard) = PROGRESS.get_or_init(Default::default).lock() {
                if guard.as_ref().map(|(p, _)| *p) != found {
                    CHANGED.store(true, Ordering::Release);
                }
                *guard = found.map(|p| (p, Instant::now()));
            }
            BUSY.store(false, Ordering::Release);
        })
        .is_err()
    {
        BUSY.store(false, Ordering::Release);
    }
}

#[derive(Default)]
struct Scan {
    progress: Option<f32>,
}

fn scan() -> Option<f32> {
    let mut scan = Scan::default();
    unsafe {
        let _ = EnumWindows(Some(window), LPARAM((&mut scan as *mut Scan) as isize));
    }
    scan.progress
}

unsafe extern "system" fn window(hwnd: HWND, context: LPARAM) -> BOOL {
    unsafe {
        // New Windows 11 Explorer operations are sometimes hosted in a
        // DirectUI window instead of the older #32770 dialog class. Identify
        // them by the owning explorer.exe process and their genuine progress
        // source, rather than relying on one shell window class.
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return BOOL(1);
        };
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        if result.is_err()
            || !String::from_utf16_lossy(&path[..len as usize])
                .to_ascii_lowercase()
                .ends_with("\\explorer.exe")
        {
            return BOOL(1);
        }
        let _ = EnumChildWindows(Some(hwnd), Some(child), context);
        // Current Windows 11 copy dialogs can draw their progress graph without
        // a native msctls_progress32 child. Their window title still carries
        // the Shell's own percentage (for example "41% complete").
        let scan = &mut *(context.0 as *mut Scan);
        if scan.progress.is_none() {
            let mut title = [0u16; 128];
            let len = GetWindowTextW(hwnd, &mut title) as usize;
            scan.progress = parse_copy_dialog_progress(&String::from_utf16_lossy(&title[..len.min(title.len())]));
        }
        BOOL(1)
    }
}

unsafe extern "system" fn child(hwnd: HWND, context: LPARAM) -> BOOL {
    unsafe {
        if !has_class(hwnd, "msctls_progress32") {
            return BOOL(1);
        }
        // These are the standard progress-bar messages. With a null lParam,
        // the range packs minimum in the high word and maximum in the low word.
        const PBM_GETRANGE: u32 = 0x407;
        const PBM_GETPOS: u32 = 0x408;
        let mut range = 0usize;
        let mut position = 0usize;
        let range_ok = SendMessageTimeoutW(
            hwnd,
            PBM_GETRANGE,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            150,
            Some(&mut range),
        );
        let position_ok = SendMessageTimeoutW(
            hwnd,
            PBM_GETPOS,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            150,
            Some(&mut position),
        );
        if range_ok.0 == 0 || position_ok.0 == 0 {
            return BOOL(1);
        }
        let min = (range >> 16) & 0xffff;
        let max = range & 0xffff;
        if max <= min || position < min || position > max {
            return BOOL(1);
        }
        let fraction = (position - min) as f32 / (max - min) as f32;
        let scan = &mut *(context.0 as *mut Scan);
        scan.progress = Some(fraction);
        BOOL(1)
    }
}

unsafe fn has_class(hwnd: HWND, expected: &str) -> bool {
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    String::from_utf16_lossy(&class[..len.min(class.len())]) == expected
}

fn parse_copy_dialog_progress(title: &str) -> Option<f32> {
    let trimmed = title.trim();
    let (number, rest) = trimmed.split_once('%')?;
    let percent: u8 = number.trim().parse().ok()?;
    (percent <= 100 && rest.trim().to_ascii_lowercase().starts_with("complete"))
        .then_some(percent as f32 / 100.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn uses_only_shell_copy_dialog_percentage() {
        assert_eq!(super::parse_copy_dialog_progress("41% complete"), Some(0.41));
        assert_eq!(super::parse_copy_dialog_progress("100% complete"), Some(1.0));
        assert_eq!(super::parse_copy_dialog_progress("41% downloaded"), None);
        assert_eq!(super::parse_copy_dialog_progress("10% complete"), Some(0.10));
        assert_eq!(super::parse_copy_dialog_progress("140% complete"), None);
    }

    #[test]
    #[ignore = "requires an active Windows Explorer file copy dialog"]
    fn live_explorer_copy_progress() {
        let progress = super::scan().expect("Explorer copy progress");
        assert!((0.0..=1.0).contains(&progress));
    }
}
