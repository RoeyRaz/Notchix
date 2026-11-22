//! Optional weather lookup. The location lives locally in weather.txt and is
//! never inferred from an IP address or the operating system.
use std::sync::{Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct Weather {
    pub city: String,
    pub temperature: Option<f32>,
    pub feels_like: Option<f32>,
    pub humidity: Option<u32>,
    pub code: u32,
}

static STATE: OnceLock<Mutex<(Weather, Option<Instant>)>> = OnceLock::new();
static BUSY: AtomicBool = AtomicBool::new(false);

fn state() -> &'static Mutex<(Weather, Option<Instant>)> {
    STATE.get_or_init(|| Mutex::new((Weather::default(), None)))
}

pub fn snapshot() -> Weather {
    state().lock().map(|s| s.0.clone()).unwrap_or_default()
}

pub fn description(code: u32) -> &'static str {
    match code {
        0 => "Clear sky",
        1 | 2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61..=67 | 80..=82 => "Rain",
        71..=77 | 85 | 86 => "Snow",
        95..=99 => "Thunderstorm",
        _ => "Current conditions",
    }
}

pub fn refresh() {
    let path = crate::store::data_dir().join("weather.txt");
    if !path.exists() {
        let _ = std::fs::write(&path, "# City|Latitude|Longitude (coordinates are optional until you configure them)\n# Example: Tel Aviv|32.0853|34.7818\n");
    }
    let config = std::fs::read_to_string(path).unwrap_or_default();
    let Some(line) = config.lines().map(str::trim).find(|s| !s.is_empty() && !s.starts_with('#')) else { return; };
    let mut parts = line.split('|');
    let (Some(city), Some(lat), Some(lon)) = (parts.next(), parts.next(), parts.next()) else { return; };
    let (Ok(lat), Ok(lon)) = (lat.trim().parse::<f64>(), lon.trim().parse::<f64>()) else { return; };
    if city.trim().is_empty() || !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) { return; }
    if state().lock().map(|s| s.1.is_some_and(|t| t.elapsed() < Duration::from_secs(1800))).unwrap_or(false) { return; }
    if BUSY.swap(true, Ordering::SeqCst) { return; }
    let city = city.trim().chars().take(64).collect::<String>();
    std::thread::spawn(move || {
        let url = format!("https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code");
        let script = format!("$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; try {{ $w=Invoke-RestMethod -Uri '{url}' -TimeoutSec 15; Write-Output ('OK|'+$w.current.temperature_2m+'|'+$w.current.apparent_temperature+'|'+$w.current.relative_humidity_2m+'|'+$w.current.weather_code) }} catch {{ Write-Output 'ERROR' }}");
        let result = crate::sys::powershell(&script).unwrap_or_default();
        if let Some(data) = result.lines().find(|s| s.trim().starts_with("OK|")) {
            let fields: Vec<_> = data.trim().split('|').collect();
            if fields.len() == 5 {
                if let (Ok(temp), Ok(feels), Ok(humidity), Ok(code)) = (
                    fields[1].parse::<f32>(), fields[2].parse::<f32>(),
                    fields[3].parse::<u32>(), fields[4].parse::<u32>(),
                ) {
                    if let Ok(mut s) = state().lock() {
                        s.0 = Weather { city, temperature: Some(temp), feels_like: Some(feels), humidity: Some(humidity), code };
                        s.1 = Some(Instant::now());
                    }
                }
            }
        }
        BUSY.store(false, Ordering::SeqCst);
    });
}
