//! Optional read-only qBittorrent Web API live activity.
//! Requires qBittorrent >= 5.2 and a locally configured API key.
//! Only connects to loopback and never persists or logs the key.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq)]
pub struct Download {
    pub name: String,
    pub percent: f32,
    pub speed: u64,
    pub eta: Option<u64>,
    pub count: usize,
}

static STATUS: OnceLock<Mutex<Option<(Download, Instant)>>> = OnceLock::new();

pub fn current() -> Option<Download> {
    let guard = STATUS.get()?.lock().ok()?;
    let (item, seen) = guard.as_ref()?;
    (seen.elapsed() < Duration::from_secs(12)).then(|| item.clone())
}

fn fetch(key: &str, port: u16) -> Option<Download> {
    // Local API only: never forward a credential to a network host.
    let endpoint = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let mut socket =
        TcpStream::connect_timeout(&endpoint.into(), Duration::from_millis(850)).ok()?;
    socket
        .set_read_timeout(Some(Duration::from_millis(900)))
        .ok()?;
    socket
        .set_write_timeout(Some(Duration::from_millis(900)))
        .ok()?;
    let request = format!("GET /api/v2/torrents/info?filter=downloading HTTP/1.0\r\nHost: localhost:{port}\r\nOrigin: http://localhost:{port}\r\nAuthorization: Bearer {key}\r\nConnection: close\r\n\r\n");
    socket.write_all(request.as_bytes()).ok()?;
    // qBittorrent can reply with `Connection: keep-alive` even to our
    // `Connection: close` request. Read the declared body length instead of
    // waiting for EOF, which would discard an otherwise complete response.
    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    let split = loop {
        if let Some(split) = raw.windows(4).position(|b| b == b"\r\n\r\n") {
            break split;
        }
        if raw.len() >= 16 * 1024 {
            return None;
        }
        let n = socket.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..n]);
    };
    let header = std::str::from_utf8(&raw[..split]).ok()?;
    if !header.starts_with("HTTP/1.0 200 ") && !header.starts_with("HTTP/1.1 200 ") {
        return None;
    }
    let length = header
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("Content-Length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .next()?;
    if length > 512 * 1024 {
        return None;
    }
    let body_start = split + 4;
    let body_end = body_start.checked_add(length)?;
    while raw.len() < body_end {
        let n = socket.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..n]);
    }
    let data: serde_json::Value = serde_json::from_slice(&raw[body_start..body_end]).ok()?;
    let torrents = data.as_array()?;
    // The most active torrent gets the primary card. The count stays visible.
    let best = torrents
        .iter()
        .max_by_key(|item| item["dlspeed"].as_u64().unwrap_or(0))?;
    let speed = best["dlspeed"].as_u64().unwrap_or(0);
    let progress = best["progress"].as_f64().unwrap_or(0.0) as f32;
    if progress >= 1.0 {
        return None;
    }
    let eta = best["eta"].as_u64().filter(|v| *v < 8_640_000);
    let name = best["name"]
        .as_str()
        .unwrap_or("Torrent")
        .chars()
        .take(65)
        .collect();
    Some(Download {
        name,
        percent: progress.clamp(0.0, 1.0),
        speed,
        eta,
        count: torrents.len(),
    })
}

pub fn start() {
    let Ok(key) = std::env::var("NOCK_QBIT_API_KEY") else {
        return;
    };
    // API keys are qbt_ + 28 alphanumerics. Reject unexpected header bytes.
    if key.len() != 32
        || !key.starts_with("qbt_")
        || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return;
    }
    let port = std::env::var("NOCK_QBIT_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(8080);
    std::thread::spawn(move || loop {
        let item = fetch(&key, port).map(|item| (item, Instant::now()));
        if let Ok(mut guard) = STATUS.get_or_init(Default::default).lock() {
            *guard = item;
        }
        std::thread::sleep(Duration::from_secs(3));
    });
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires qBittorrent with an active download and a local API key"]
    fn live_download_progress() {
        let key = std::env::var("NOCK_QBIT_API_KEY").expect("qBittorrent API key");
        let port = std::env::var("NOCK_QBIT_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(8080);
        let download = super::fetch(&key, port).expect("active qBittorrent download");
        assert!((0.0..1.0).contains(&download.percent));
        assert!(download.count > 0);
    }
}
