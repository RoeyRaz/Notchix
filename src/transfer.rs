//! Lightweight, best-effort transfer activity detection across common apps.
//! Windows does not provide a universal per-download percentage API. This
//! module reports observed process I/O, never a fabricated percent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[derive(Clone, Default, PartialEq)]
pub struct Activity {
    pub app: String,
    pub bytes_per_second: u64,
}

#[derive(Default)]
pub struct Monitor {
    previous: HashMap<u32, (u64, u64, String)>,
    last: Option<Instant>,
    active: Option<Activity>,
    quiet_samples: u8,
    download_files: HashMap<PathBuf, u64>,
}

static MONITOR: OnceLock<Mutex<Monitor>> = OnceLock::new();

pub fn current() -> Option<Activity> {
    MONITOR.get().and_then(|m| m.lock().ok()?.active.clone())
}

fn watched(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "steam.exe" | "steamwebhelper.exe" | "steamservice.exe" | "steamerrorreporter.exe" => Some("Steam"),
        // Calls, video playback and caches generate I/O too. Do not show
        // generic Discord/Explorer activity as if it were a download.
        "zen.exe" => Some("Zen Browser"),
        "chrome.exe" | "msedge.exe" | "firefox.exe" | "brave.exe" | "opera.exe" => Some("Browser"),
        "epicgameslauncher.exe" => Some("Epic Games"),
        "battle.net.exe" | "agent.exe" => Some("Battle.net"),
        _ => None,
    }
}

/// Browser cache and video streaming should never produce an Island activity.
/// An actively growing .part/.crdownload in Downloads is stronger evidence of
/// a real user download. We only inspect filename and size, not file content.
fn growing_browser_downloads(previous: &mut HashMap<PathBuf, u64>) -> Option<u64> {
    let root = PathBuf::from(std::env::var_os("USERPROFILE")?).join("Downloads");
    growing_browser_downloads_in(&root, previous)
}

fn growing_browser_downloads_in(root: &Path, previous: &mut HashMap<PathBuf, u64>) -> Option<u64> {
    let mut current = HashMap::new();
    let mut growth = 0u64;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten().take(1500) {
            let path = entry.path();
            let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if !(name.ends_with(".part") || name.ends_with(".crdownload") || name.ends_with(".download")) { continue; }
            if let Ok(metadata) = entry.metadata() {
                if !metadata.is_file() { continue; }
                let size = metadata.len();
                if let Some(before) = previous.get(&path) {
                    growth = growth.saturating_add(size.saturating_sub(*before));
                }
                current.insert(path, size);
            }
        }
    }
    *previous = current;
    (growth > 0).then_some(growth)
}

/// Called only once every two seconds on the existing window timer. Process
/// enumeration is cheap, and processes are opened read-only for IO counters.
pub fn sample() -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::System::Threading::*;
    use windows::Win32::Foundation::CloseHandle;
    let Ok(mut monitor) = MONITOR.get_or_init(Default::default).lock() else { return false };
    let now = Instant::now();
    let elapsed = monitor.last.map(|t| now.duration_since(t).as_secs_f64()).unwrap_or(0.0);
    let mut next = HashMap::new();
    let mut by_app: HashMap<String, u64> = HashMap::new();
    unsafe {
        if let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            if Process32FirstW(snapshot, &mut e).is_ok() {
                loop {
                    let len = e.szExeFile.iter().position(|v| *v == 0).unwrap_or(e.szExeFile.len());
                    let name = String::from_utf16_lossy(&e.szExeFile[..len]);
                    if let Some(label) = watched(&name) {
                        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, e.th32ProcessID) {
                            let mut io = IO_COUNTERS::default();
                            if GetProcessIoCounters(handle, &mut io).is_ok() {
                                // Steam often downloads to buffers before
                                // writing to disk. OtherTransferCount tracks
                                // asynchronous I/O such as network transfers,
                                // which can remain active between disk flushes.
                                let (write, other) = (io.WriteTransferCount, io.OtherTransferCount);
                                if let Some((old_write, old_other, old_label)) = monitor.previous.get(&e.th32ProcessID) {
                                    if old_label == label && elapsed > 0.5 {
                                        let delta = write.saturating_sub(*old_write).max(other.saturating_sub(*old_other));
                                        *by_app.entry(label.into()).or_default() += delta;
                                    }
                                }
                                next.insert(e.th32ProcessID, (write, other, label.into()));
                            }
                            let _ = CloseHandle(handle);
                        }
                    }
                    if Process32NextW(snapshot, &mut e).is_err() { break; }
                }
            }
            let _ = CloseHandle(snapshot);
        }
    }
    monitor.previous = next;
    monitor.last = Some(now);
    let browser_download_growth = growing_browser_downloads(&mut monitor.download_files);
    let browser_running = monitor.previous.values().any(|(_, _, label)| label == "Zen Browser" || label == "Browser");
    // Process I/O alone cannot distinguish an actual file download from
    // YouTube, Discord voice, tab cache writes, etc. Browser activity is
    // eligible only while a real partial download file is growing.
    by_app.remove("Browser");
    by_app.remove("Zen Browser");
    if let Some(bytes) = browser_download_growth.filter(|_| browser_running) {
        // The same .part extension is used by Zen and Firefox. If both are
        // running, file size alone cannot attribute the download to either.
        by_app.insert("Browser download".into(), bytes);
    }
    // A little background IO is normal. Require meaningful sustained writes.
    let strongest = by_app.into_iter().max_by_key(|(_, bytes)| *bytes);
    let next_activity = strongest.and_then(|(app, bytes)| {
        let rate = (bytes as f64 / elapsed.max(0.5)) as u64;
        let threshold = if app == "Browser download" { 1024 } else { 64 * 1024 };
        (rate >= threshold).then_some(Activity { app, bytes_per_second: rate })
    });
    let old = monitor.active.clone();
    if let Some(activity) = next_activity {
        monitor.active = Some(activity);
        monitor.quiet_samples = 0;
    } else {
        monitor.quiet_samples = monitor.quiet_samples.saturating_add(1);
        if monitor.quiet_samples >= 2 { monitor.active = None; }
    }
    old != monitor.active
}

pub fn rate_label(bytes_per_second: u64) -> String {
    if bytes_per_second >= 1024 * 1024 {
        format!("{:.1} MB/s", bytes_per_second as f64 / 1_048_576.0)
    } else {
        format!("{} KB/s", bytes_per_second / 1024)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn browser_download_requires_a_growing_partial_file() {
        use std::collections::HashMap;
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("notchix-browser-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let partial = dir.join("test.bin.part");
        let cache = dir.join("video-cache.bin");
        std::fs::write(&partial, [0u8; 16]).unwrap();
        std::fs::write(&cache, [0u8; 2048]).unwrap();
        let mut previous = HashMap::new();
        assert_eq!(super::growing_browser_downloads_in(&dir, &mut previous), None);
        let mut file = std::fs::OpenOptions::new().append(true).open(&partial).unwrap();
        file.write_all(&[0u8; 2048]).unwrap();
        file.flush().unwrap();
        drop(file);
        assert_eq!(super::growing_browser_downloads_in(&dir, &mut previous), Some(2048));
        assert_eq!(super::growing_browser_downloads_in(&dir, &mut previous), None);
        std::fs::remove_file(partial).unwrap();
        std::fs::remove_file(cache).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
