//! Optional, on-demand LRCLIB lookup. No API key, browser, or idle polling.
//! Only song title and artist are sent to the lyrics service when requested.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Clone, Default)]
pub struct Line {
    pub at: Option<f64>,
    pub text: String,
}

#[derive(Clone, Default)]
pub struct ResultData {
    pub key: String,
    pub lines: Vec<Line>,
    pub loading: bool,
    pub synced: bool,
    pub message: String,
}

pub static STATE: Mutex<Option<ResultData>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn key(title: &str, artist: &str) -> String { format!("{}|{}", title.trim(), artist.trim()) }

fn decode(content: &str) -> (Vec<Line>, bool) {
    let content = content.trim_start_matches('\u{feff}');
    let (synced, body) = match content.chars().next() {
        Some('S') => (true, &content[1..]),
        Some('P') => (false, &content[1..]),
        _ => return (Vec::new(), false),
    };
    let lines = body.lines().filter_map(|raw| {
        let raw = raw.trim_end_matches('\r');
        if raw.trim().is_empty() { return None; }
        if synced {
            let (stamp, text) = raw.strip_prefix('[')?.split_once(']')?;
            let (min, sec) = stamp.split_once(':')?;
            let at = min.parse::<f64>().ok()? * 60.0 + sec.parse::<f64>().ok()?;
            Some(Line { at: Some(at), text: text.trim().to_string() })
        } else {
            Some(Line { at: None, text: raw.to_string() })
        }
    }).take(500).collect();
    (lines, synced)
}

pub fn request(title: &str, artist: &str) {
    let requested_key = key(title, artist);
    if title.trim().is_empty() || artist.trim().is_empty() { return; }
    {
        let current = STATE.lock().unwrap();
        if current.as_ref().is_some_and(|v| v.key == requested_key) { return; }
    }
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    *STATE.lock().unwrap() = Some(ResultData { key: requested_key.clone(), loading: true, message: "Finding lyrics...".into(), ..Default::default() });
    let title = title.to_owned();
    let artist = artist.to_owned();
    std::thread::spawn(move || {
        let path = crate::store::data_dir().join(format!("lyrics-{}-{generation}.tmp", std::process::id()));
        let safe_path = path.to_string_lossy().replace('\'', "''");
        let title = title.replace('\'', "''");
        let artist = artist.replace('\'', "''");
        let cmd = format!(
            "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; \
             try {{ $u='https://lrclib.net/api/search?track_name=' + [uri]::EscapeDataString('{title}') + '&artist_name=' + [uri]::EscapeDataString('{artist}'); \
             $r=Invoke-RestMethod -Uri $u -TimeoutSec 15; \
             $entry=@($r) | Where-Object {{ $_.syncedLyrics -or $_.plainLyrics }} | Select-Object -First 1; \
             if ($entry) {{ $value=if ($entry.syncedLyrics) {{ 'S' + $entry.syncedLyrics }} else {{ 'P' + $entry.plainLyrics }}; \
             [IO.File]::WriteAllText('{safe_path}', $value, [Text.Encoding]::UTF8) }} }} catch {{ }}"
        );
        let _ = crate::sys::powershell(&cmd);
        let (lines, synced) = std::fs::read_to_string(&path).ok().map(|s| decode(&s)).unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        if GENERATION.load(Ordering::SeqCst) != generation { return; }
        let message = if lines.is_empty() { "No lyrics found for this track".into() } else { String::new() };
        *STATE.lock().unwrap() = Some(ResultData { key: requested_key, lines, synced, loading: false, message });
    });
}

pub fn snapshot() -> Option<ResultData> { STATE.lock().ok()?.clone() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lrc_timestamps() {
        let (lines, synced) = decode("S[00:12.50]First\n[01:03.00]Second");
        assert!(synced);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].at, Some(12.5));
        assert_eq!(lines[1].text, "Second");
    }
}
