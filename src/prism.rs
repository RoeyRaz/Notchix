//! Read-only live download activity from Prism Download Manager.
//! Prism publishes its data in %APPDATA%\Prism\prism.db. We only read rows
//! whose persisted status is actively downloading; no process-I/O guesswork.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq)]
pub struct Download {
    pub name: String,
    pub percent: Option<f32>,
    pub speed: u64,
    pub eta: Option<u64>,
    pub count: usize,
    pub state: String,
}

static STATUS: OnceLock<Mutex<Option<(Download, Instant)>>> = OnceLock::new();
static BUSY: AtomicBool = AtomicBool::new(false);
static CHANGED: AtomicBool = AtomicBool::new(false);

pub fn current() -> Option<Download> {
    let guard = STATUS.get()?.lock().ok()?;
    let (item, seen) = guard.as_ref()?;
    (seen.elapsed() < Duration::from_secs(8)).then(|| item.clone())
}

#[cfg(windows)]
mod platform {
    use super::Download;
    use rusqlite::{Connection, OpenFlags};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    #[derive(Default)]
    struct Monitor {
        last: Option<Instant>,
        previous: HashMap<String, u64>,
    }

    static MONITOR: OnceLock<Mutex<Monitor>> = OnceLock::new();

    fn data_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::with_capacity(2);
        if let Some(roaming) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(roaming).join("Prism"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Prism").join("data"));
        }
        dirs
    }

    fn prism_is_running(pid: u32) -> bool {
        unsafe {
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let mut path = [0u16; 520];
            let mut len = path.len() as u32;
            let ok = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut len,
            )
            .is_ok();
            let _ = CloseHandle(process);
            ok && Path::new(&String::from_utf16_lossy(&path[..len as usize]))
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("prism.exe"))
        }
    }

    fn active_pid(dir: &Path) -> Option<u32> {
        let info: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("ipc.json")).ok()?).ok()?;
        let pid = info.get("pid")?.as_u64()?.try_into().ok()?;
        prism_is_running(pid).then_some(pid)
    }

    fn read_active(
        dir: &Path,
        previous: &mut HashMap<String, u64>,
        elapsed: f64,
    ) -> Option<Download> {
        let db = Connection::open_with_flags(
            dir.join("prism.db"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        db.busy_timeout(Duration::from_millis(75)).ok()?;
        let mut stmt = db
            .prepare(
                "SELECT id, file_name, downloaded, size, status FROM downloads \
             WHERE status IN ('connecting','downloading','retrying') AND trashed=0 \
             ORDER BY downloaded DESC LIMIT 128",
            )
            .ok()?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?.max(0) as u64,
                    row.get::<_, Option<i64>>(3)?
                        .filter(|size| *size > 0)
                        .map(|size| size as u64),
                    row.get::<_, String>(4)?,
                ))
            })
            .ok()?;
        let mut current = Vec::new();
        for row in rows.flatten() {
            current.push(row);
        }
        if current.is_empty() {
            previous.clear();
            return None;
        }

        let count = current.len();
        let best = current.iter().max_by_key(|(id, _, downloaded, _, _)| {
            downloaded.saturating_sub(*previous.get(id).unwrap_or(downloaded))
        })?;
        let (id, name, downloaded, size, status) = best;
        let speed = previous
            .get(id)
            .map(|before| downloaded.saturating_sub(*before))
            .map(|delta| (delta as f64 / elapsed.max(0.5)) as u64)
            .unwrap_or(0);
        let percent = size.map(|total| (*downloaded as f64 / total as f64).clamp(0.0, 1.0) as f32);
        let eta = size
            .filter(|total| speed > 0 && *downloaded < *total)
            .map(|total| (total - *downloaded) / speed);
        let mut next = HashMap::with_capacity(count);
        for (id, _, downloaded, _, _) in &current {
            next.insert(id.clone(), *downloaded);
        }
        *previous = next;
        Some(Download {
            name: name.chars().take(54).collect(),
            percent,
            speed,
            eta,
            count,
            state: status.clone(),
        })
    }

    pub fn sample() -> bool {
        let Ok(mut monitor) = MONITOR.get_or_init(Default::default).lock() else {
            return false;
        };
        let now = Instant::now();
        let elapsed = monitor
            .last
            .map(|last| now.duration_since(last).as_secs_f64())
            .unwrap_or(2.0);
        monitor.last = Some(now);
        let mut found = None;
        for dir in data_dirs() {
            if active_pid(&dir).is_none() {
                continue;
            }
            found = read_active(&dir, &mut monitor.previous, elapsed);
            if found.is_some() {
                break;
            }
        }
        if found.is_none() {
            monitor.previous.clear();
        }
        if let Ok(mut status) = super::STATUS.get_or_init(Default::default).lock() {
            let old = status.as_ref().map(|(item, _)| item.clone());
            *status = found.map(|item| (item, now));
            return old != status.as_ref().map(|(item, _)| item.clone());
        }
        false
    }
}

/// Run the Prism database read off the UI thread, reusing the existing
/// two-second transfer timer. A single atomic guard prevents overlapping reads.
pub fn sample_async() {
    #[cfg(windows)]
    {
        if BUSY.swap(true, Ordering::AcqRel) {
            return;
        }
        if std::thread::Builder::new()
            .name("prism-progress".into())
            .spawn(|| {
                if platform::sample() {
                    CHANGED.store(true, Ordering::Release);
                }
                BUSY.store(false, Ordering::Release);
            })
            .is_err()
        {
            BUSY.store(false, Ordering::Release);
        }
    }
}

#[cfg(not(windows))]
pub fn sample_async() {}

pub fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::AcqRel)
}
