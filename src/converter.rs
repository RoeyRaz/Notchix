//! On-demand media conversion through a locally installed FFmpeg.
//! UI progress is reported only when ffprobe supplies a real duration.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const GROUPS: [&str; 3] = ["Video", "Audio", "Image"];
pub const VIDEO: &[(&str, &str)] = &[
    ("MP4", "mp4"),
    ("MKV", "mkv"),
    ("WebM", "webm"),
    ("MOV", "mov"),
    ("GIF", "gif"),
];
pub const AUDIO: &[(&str, &str)] = &[
    ("MP3", "mp3"),
    ("WAV", "wav"),
    ("FLAC", "flac"),
    ("M4A", "m4a"),
    ("AAC", "aac"),
    ("OGG", "ogg"),
];
pub const IMAGE: &[(&str, &str)] = &[
    ("PNG", "png"),
    ("JPEG", "jpg"),
    ("WebP", "webp"),
    ("BMP", "bmp"),
    ("TIFF", "tiff"),
];

pub fn formats(group: u8) -> &'static [(&'static str, &'static str)] {
    match group {
        1 => AUDIO,
        2 => IMAGE,
        _ => VIDEO,
    }
}

pub fn group_for_path(path: &Path) -> u8 {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus" | "wma" | "aiff"
    ) {
        1
    } else if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "gif" | "heic"
    ) {
        2
    } else {
        0
    }
}

pub fn is_supported(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "mp4"
            | "mkv"
            | "webm"
            | "mov"
            | "gif"
            | "avi"
            | "m4v"
            | "mpg"
            | "mpeg"
            | "wmv"
            | "flv"
            | "ts"
            | "mp3"
            | "wav"
            | "flac"
            | "m4a"
            | "aac"
            | "ogg"
            | "opus"
            | "wma"
            | "aiff"
            | "png"
            | "jpg"
            | "jpeg"
            | "webp"
            | "bmp"
            | "tif"
            | "tiff"
            | "heic"
    )
}

pub fn destination(input: &Path, extension: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("converted");
    for n in 1..=999u32 {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!("_{n}")
        };
        let candidate = input.with_file_name(format!("{stem}_converted{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    input.with_file_name(format!(
        "{stem}_converted_{}.{}",
        std::process::id(),
        extension
    ))
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub progress: Option<f32>,
    pub result: Option<Result<String, String>>,
}

pub fn start(input: PathBuf, output: PathBuf, shared: Arc<Mutex<Snapshot>>, hwnd: isize) {
    std::thread::spawn(move || {
        let duration_s = probe_duration(&input);
        let result = run_ffmpeg(&input, &output, duration_s, &shared, hwnd);
        if let Ok(mut state) = shared.lock() {
            state.result = Some(result);
        }
        post(hwnd);
    });
}

pub fn probe_duration(path: &Path) -> Option<f64> {
    use std::os::windows::process::CommandExt;
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .creation_flags(0x08000000)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

fn run_ffmpeg(
    input: &Path,
    output: &Path,
    duration_s: Option<f64>,
    shared: &Arc<Mutex<Snapshot>>,
    hwnd: isize,
) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-nostdin",
            "-n",
            "-progress",
            "pipe:1",
            "-nostats",
            "-i",
        ])
        .arg(input)
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|_| {
            "FFmpeg was not found. Install FFmpeg and add ffmpeg.exe to PATH.".to_string()
        })?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Err("Could not read conversion progress from FFmpeg.".into());
    };
    let mut last_post = Instant::now();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(value) = line
            .strip_prefix("out_time_us=")
            .and_then(|s| s.parse::<u64>().ok())
        {
            let progress = duration_s
                .map(|duration| ((value as f64 / 1_000_000.0) / duration).clamp(0.0, 0.995) as f32);
            if let Ok(mut state) = shared.lock() {
                state.progress = progress;
            }
            if last_post.elapsed() >= Duration::from_millis(120) {
                post(hwnd);
                last_post = Instant::now();
            }
        }
    }
    let status = child
        .wait()
        .map_err(|_| "FFmpeg stopped unexpectedly.".to_string())?;
    if status.success() {
        if let Ok(mut state) = shared.lock() {
            state.progress = Some(1.0);
        }
        Ok(output.to_string_lossy().into_owned())
    } else {
        Err("This FFmpeg build could not decode the source or encode the selected format.".into())
    }
}

fn post(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut _)),
            crate::app::WM_CONVERTER_UPDATE,
            WPARAM(0),
            LPARAM(0),
        );
    }
}
