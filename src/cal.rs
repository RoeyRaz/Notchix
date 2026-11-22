//! Calendar hints: reads a read-only ICS link (Google "secret address in iCal format",
//! Outlook "published calendar") so the pill can say "Standup 5m" before a meeting.
//! Off unless `calendar_url` is set in config.ini. The link is fetched at start and
//! about once an hour, never on a tighter loop.
//!
//! All times are "local epoch" seconds: the local wall clock read as if it were UTC.
// DAILY, WEEKLY, MONTHLY and YEARLY rules are supported, including COUNT,
// BYDAY, BYMONTHDAY, BYMONTH, UNTIL, and EXDATE. TZID conversion is limited
// (TZID times are taken as this PC's time,
// UTC times drift an hour across a DST change). Upgrade path: a real iCal crate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub const WM_CALENDAR_READY: u32 = 0x8000 + 43;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub start: i64,
    pub end: Option<i64>,
    pub title: String,
    pub link: Option<String>,
    pub all_day: bool,
}

/// Upcoming occurrences from the last fetch.
pub static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());
#[derive(Clone, Debug)]
pub struct LabeledEvent {
    pub event: Event,
    pub calendar: String,
    pub color_index: usize,
}

/// Same occurrences with their source names, for the Home agenda.
pub static LABELED_EVENTS: Mutex<Vec<LabeledEvent>> = Mutex::new(Vec::new());
static BUSY: AtomicBool = AtomicBool::new(false);

// Retain active meetings across hourly feed refreshes so their progress rim
// does not vanish ten minutes after they start.
const LOOKBACK: i64 = 86400;
// Include the coming fortnight, so a weekend does not make a connected
// college/work calendar appear empty just because Monday is >36h away.
const LOOKAHEAD: i64 = 14 * 86400;

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    if m <= 2 { y += 1; }
    (y, m, d)
}

/// 0 = Sunday.
fn weekday(day: i64) -> i64 {
    (day + 4).rem_euclid(7)
}

/// (local epoch now, seconds the local clock is ahead of UTC).
pub fn now() -> (i64, i64) {
    let (y, mo, d, _, h, mi, s, _) = crate::sys::local_time();
    let local = days_from_civil(y as i64, mo as i64, d as i64) * 86400 + (h * 3600 + mi * 60 + s) as i64;
    let utc = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_secs() as i64).unwrap_or(local);
    (local, local - utc)
}

/// `20261019T100000Z` / `20261019T100000` -> local epoch; None for all-day dates.
fn parse_dt(v: &str, off: i64) -> Option<i64> {
    let v = v.trim();
    if v.len() == 8 {
        let y = v.get(0..4)?.parse().ok()?;
        let m = v.get(4..6)?.parse().ok()?;
        let d = v.get(6..8)?.parse().ok()?;
        return Some(days_from_civil(y, m, d) * 86400);
    }
    if v.len() < 15 || !v.is_char_boundary(8) || v.as_bytes()[8] != b'T' {
        return None;
    }
    let n = |a: usize, b: usize| v.get(a..b)?.parse::<i64>().ok();
    let t = days_from_civil(n(0, 4)?, n(4, 6)?, n(6, 8)?) * 86400 + n(9, 11)? * 3600 + n(11, 13)? * 60 + n(13, 15)?;
    Some(if v.ends_with('Z') { t + off } else { t })
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// A meeting link (Zoom, Meet, Teams, Webex) found in the text, else None.
fn meeting_link(text: &str) -> Option<String> {
    const HOSTS: [&str; 5] = ["zoom.us", "meet.google.com", "teams.microsoft.com", "teams.live.com", "webex.com"];
    let mut rest = text;
    while let Some(i) = rest.find("https://") {
        let tail = &rest[i..];
        let end = tail.find(|c: char| c.is_whitespace() || matches!(c, '\\' | '<' | '>' | '"')).unwrap_or(tail.len());
        let url = &tail[..end];
        if HOSTS.iter().any(|h| url.contains(h)) {
            return Some(url.to_string());
        }
        rest = &tail[8..];
    }
    None
}

#[derive(Default)]
struct Raw {
    uid: String,
    start: Option<i64>,
    end: Option<i64>,
    all_day: bool,
    title: String,
    text: String,
    url: String,
    rrule: String,
    exdates: Vec<i64>,
    recurrence_id: Option<i64>,
    cancelled: bool,
}

fn matches_byday(spec: &str, day: i64, month_day: i64, month_len: i64) -> bool {
    let (prefix, suffix) = spec.split_at(spec.len().saturating_sub(2));
    let wanted = match suffix { "SU" => 0, "MO" => 1, "TU" => 2, "WE" => 3, "TH" => 4, "FR" => 5, "SA" => 6, _ => return false };
    if weekday(day) != wanted { return false; }
    if prefix.is_empty() { return true; }
    match prefix.parse::<i64>() {
        Ok(n) if n > 0 => (month_day - 1) / 7 + 1 == n,
        Ok(n) if n < 0 => -((month_len - month_day) / 7 + 1) == n,
        _ => false,
    }
}

/// The occurrences between a little ago and 14 days ahead, soonest first.
pub fn parse(ics: &str, now: i64, off: i64) -> Vec<Event> {
    let mut unfolded: Vec<String> = Vec::new();
    for l in ics.lines() {
        if l.starts_with(' ') || l.starts_with('\t') {
            if let Some(last) = unfolded.last_mut() {
                last.push_str(&l[1..]);
            }
        } else {
            unfolded.push(l.to_string());
        }
    }
    let mut raws: Vec<Raw> = Vec::new();
    let mut cur: Option<Raw> = None;
    for l in &unfolded {
        if l == "BEGIN:VEVENT" {
            cur = Some(Raw::default());
        } else if l == "END:VEVENT" {
            raws.extend(cur.take());
        } else if let (Some(r), Some((head, val))) = (cur.as_mut(), l.split_once(':')) {
            let name = head.split(';').next().unwrap_or("").to_ascii_uppercase();
            let all_day = head.to_ascii_uppercase().contains("VALUE=DATE") && !head.to_ascii_uppercase().contains("DATE-TIME");
            match name.as_str() {
                "UID" => r.uid = val.to_string(),
                "DTSTART" => {
                    r.all_day = all_day || val.len() == 8;
                    r.start = parse_dt(val, off);
                }
                "DTEND" => r.end = parse_dt(val, off),
                "SUMMARY" => r.title = unescape(val),
                "DESCRIPTION" | "LOCATION" => {
                    r.text.push(' ');
                    r.text.push_str(val);
                }
                "URL" => r.url = val.to_string(),
                "RRULE" => r.rrule = val.to_string(),
                "EXDATE" => r.exdates.extend(val.split(',').filter_map(|v| parse_dt(v, off))),
                "RECURRENCE-ID" => r.recurrence_id = parse_dt(val, off),
                "STATUS" => r.cancelled = val.trim().eq_ignore_ascii_case("CANCELLED"),
                _ => {}
            }
        }
    }
    let (lo, hi) = (now - LOOKBACK, now + LOOKAHEAD);
    // a moved instance of a repeating meeting replaces the original day
    let moved: Vec<(&str, i64)> = raws.iter().filter_map(|r| r.recurrence_id.map(|t| (r.uid.as_str(), t.div_euclid(86400)))).collect();
    let mut out = Vec::new();
    for r in raws.iter().filter(|r| !r.cancelled) {
        let Some(start) = r.start else { continue };
        let link = meeting_link(&r.text).or_else(|| meeting_link(&r.url));
        let mut push = |t: i64| {
            let visible_from = if r.all_day { now.div_euclid(86400) * 86400 } else { lo };
            if (visible_from..=hi).contains(&t) {
                let end = r.end.and_then(|end| (end > start).then_some(t + end - start));
                out.push(Event { start: t, end, title: r.title.clone(), link: link.clone(), all_day: r.all_day });
            }
        };
        let rule: Vec<(&str, &str)> = r.rrule.split(';').filter_map(|p| p.split_once('=')).collect();
        let get = |k: &str| rule.iter().find(|(a, _)| *a == k).map(|(_, v)| *v);
        let (base, tod) = (start.div_euclid(86400), start.rem_euclid(86400));
        let freq = get("FREQ");
        if r.rrule.is_empty() || !matches!(freq, Some("DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY")) {
            push(start);
            continue;
        }
        let interval = get("INTERVAL").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).max(1);
        let count = get("COUNT").and_then(|v| v.parse::<usize>().ok());
        let until = get("UNTIL").and_then(|v| {
            parse_dt(v, off).map(|t| if v.len() == 8 { t + 86399 } else { t })
        });
        let byday: Vec<&str> = get("BYDAY").map(|v| v.split(',').collect()).unwrap_or_default();
        let bymonthday: Vec<i64> = get("BYMONTHDAY").map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect()).unwrap_or_default();
        let bymonth: Vec<i64> = get("BYMONTH").map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect()).unwrap_or_default();
        let (base_y, base_m, base_d) = civil_from_days(base);
        let mut generated = 0usize;
        // COUNT starts at DTSTART, not at the visible window. Bound scanning
        // to a century even if a malformed feed supplies ancient dates.
        let first_day = base.max(hi.div_euclid(86400) - 36525);
        for day in first_day..=(hi.div_euclid(86400) + 1) {
            let (y, m, d) = civil_from_days(day);
            let month_len = days_in_month(y as i32, m as u32) as i64;
            let month_gap = (y - base_y) * 12 + m - base_m;
            let active = match freq {
                Some("DAILY") => (day - base) % interval == 0,
                Some("WEEKLY") => (day - weekday(day) - (base - weekday(base))).div_euclid(7) % interval == 0,
                Some("MONTHLY") => month_gap >= 0 && month_gap % interval == 0,
                Some("YEARLY") => (y - base_y) % interval == 0,
                _ => false,
            };
            if !active || (!bymonth.is_empty() && !bymonth.contains(&m)) { continue; }
            if !bymonthday.is_empty() && !bymonthday.iter().any(|n| *n == d || (*n < 0 && month_len + *n + 1 == d)) { continue; }
            if !byday.is_empty() && !byday.iter().any(|v| matches_byday(v, day, d, month_len)) { continue; }
            if byday.is_empty() && bymonthday.is_empty() {
                if freq == Some("WEEKLY") && weekday(day) != weekday(base) { continue; }
                if freq == Some("MONTHLY") && d != base_d { continue; }
                if freq == Some("YEARLY") && (m, d) != (base_m, base_d) { continue; }
            }
            if freq == Some("YEARLY") && bymonth.is_empty() && m != base_m { continue; }
            generated += 1;
            if count.is_some_and(|limit| generated > limit) { break; }
            let t = day * 86400 + tod;
            if until.is_some_and(|u| t > u) { break; }
            if !r.exdates.contains(&t) && !moved.contains(&(r.uid.as_str(), day)) {
                push(t);
            }
        }
    }
    out.sort_by_key(|e| e.start);
    out.dedup();
    out
}

/// Fetch the calendar in the background and replace EVENTS when it arrives.
pub fn refresh(url: &str, notify_hwnd: isize) {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut feeds: Vec<(String, String)> = Vec::new();
    if url.trim().starts_with("https://") || url.trim().starts_with("webcal://") {
        feeds.push(("Main".into(), url.trim().to_string()));
    }
    feeds.extend(crate::store::load_calendar_feeds());
    if feeds.is_empty() {
        EVENTS.lock().unwrap().clear();
        LABELED_EVENTS.lock().unwrap().clear();
        BUSY.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        if notify_hwnd != 0 {
            use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
            unsafe { let _ = PostMessageW(Some(HWND(notify_hwnd as *mut _)), WM_CALENDAR_READY, WPARAM(0), LPARAM(0)); }
        }
        return;
    }
    std::thread::spawn(move || {
        let (now, off) = now();
        let mut labeled = Vec::new();
        let mut diagnostics = Vec::new();
        for (index, (name, url)) in feeds.iter().enumerate() {
            let safe_url = url.replacen("webcal://", "https://", 1).replace('\'', "''");
            // OutFile avoids depending on the PowerShell version's
            // RawContentStream implementation, and keeps large ICS files out
            // of stdout. Remove the temporary copy after each feed.
            let download_path = crate::store::data_dir().join(format!(
                "calendar-feed-{}-{}.tmp", std::process::id(), index
            ));
            let safe_path = download_path.to_string_lossy().replace('\'', "''");
            let cmd = format!(
                "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'; \
                 [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
                 try {{ Invoke-WebRequest -UseBasicParsing -Uri '{safe_url}' -OutFile '{safe_path}' -TimeoutSec 25; \
                 Write-Output 'OK' }} catch {{ \
                 $kind = $_.Exception.GetType().Name; \
                 $status = if ($_.Exception.Response) {{ [int]$_.Exception.Response.StatusCode }} else {{ 0 }}; \
                 Write-Output ('ERROR|' + $kind + '|' + $status) }}"
            );
            let result = crate::sys::powershell(&cmd).unwrap_or_default();
            let text = std::fs::read_to_string(&download_path).ok();
            let _ = std::fs::remove_file(&download_path);
            if let Some(text) = text.filter(|s| s.contains("BEGIN:VCALENDAR")) {
                let events = parse(&text, now, off);
                diagnostics.push(format!("Feed {}: downloaded, {} upcoming timed events", index + 1, events.len()));
                labeled.extend(events.into_iter().map(|event| LabeledEvent {
                    event,
                    calendar: name.clone(),
                    color_index: index,
                }));
            } else {
                let error = result.lines().find(|line| line.starts_with("ERROR|")).unwrap_or("");
                let parts: Vec<&str> = error.split('|').collect();
                let reason = if parts.len() == 3 {
                    format!("{} (HTTP {})", parts[1], parts[2])
                } else if result.trim() == "OK" {
                    "downloaded content was not a valid iCal feed".to_string()
                } else {
                    "PowerShell did not complete the download".to_string()
                };
                diagnostics.push(format!("Feed {}: failed: {}", index + 1, reason));
            }
        }
        labeled.sort_by_key(|item| item.event.start);
        let mut merged = labeled.iter().map(|item| item.event.clone()).collect::<Vec<_>>();
        merged.dedup();
        *EVENTS.lock().unwrap() = merged;
        *LABELED_EVENTS.lock().unwrap() = labeled;
        // Intentionally exclude URL, calendar names, event titles and descriptions.
        let _ = std::fs::write(crate::store::data_dir().join("calendar-diagnostics.log"), diagnostics.join("\n"));
        BUSY.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        if notify_hwnd != 0 {
            use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
            unsafe { let _ = PostMessageW(Some(HWND(notify_hwnd as *mut _)), WM_CALENDAR_READY, WPARAM(0), LPARAM(0)); }
        }
    });
}

/// The next meeting starting within five minutes (or begun in the last minute).
pub fn soon(events: &[Event], now: i64) -> Option<&Event> {
    events.iter().find(|e| !e.all_day && (-60..=300).contains(&(e.start - now)))
}

/// Weekday (0 = Sunday) of the 1st of the month, given today's day and weekday.
pub(crate) fn first_weekday(day: u32, wday: u32) -> u32 {
    (wday + 7 - (day - 1) % 7) % 7
}

/// Weekday (0 = Sunday) of the 1st of any month.
pub(crate) fn first_of_month(y: i32, m: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    ((y + y / 4 - y / 100 + y / 400 + T[(m as usize - 1) % 12] + 1).rem_euclid(7)) as u32
}

pub(crate) fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

pub fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_format() {
        assert_eq!(fmt_time(0.0), "0:00");
        assert_eq!(fmt_time(65.4), "1:05");
        assert_eq!(fmt_time(3725.0), "1:02:05");
        assert_eq!(fmt_time(-5.0), "0:00");
    }

    #[test]
    fn month_lengths() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2026, 10), 31);
        assert_eq!(days_in_month(2026, 11), 30);
    }

    #[test]
    fn first_of_month_weekday() {
        // Thu 15 Oct 2026 -> 1 Oct 2026 was a Thursday (4)
        assert_eq!(first_weekday(15, 4), 4);
        // Sun 1 Nov 2026 -> itself
        assert_eq!(first_weekday(1, 0), 0);
        // Fri 30 Oct 2026 (5): 1st is 29 days earlier -> Thursday (4)
        assert_eq!(first_weekday(30, 5), 4);
        // any month: Oct 2026 Thu, Nov 2026 Sun, Feb 2024 Thu, Jan 2000 Sat
        assert_eq!(first_of_month(2026, 10), 4);
        assert_eq!(first_of_month(2026, 11), 0);
        assert_eq!(first_of_month(2024, 2), 4);
        assert_eq!(first_of_month(2000, 1), 6);
    }

    const ICS: &str = "BEGIN:VCALENDAR\r\n\
BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261019T100000Z\r\nSUMMARY:Standup\r\nDESCRIPTION:Join https://meet.google.com/abc-defg-hij\\nthanks\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:b\r\nDTSTART:20261019T090000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,TU\r\nEXDATE:20261020T090000\r\nSUMMARY:Sync\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:c\r\nDTSTART:20261019T110000Z\r\nSTATUS:CANCELLED\r\nSUMMARY:Gone\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:d\r\nDTSTART;VALUE=DATE:20261019\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn recurring_events_keep_real_end_times() {
        let now = days_from_civil(2026, 10, 19) * 86400 + 10 * 3600;
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20261019T090000\nDTEND:20261019T100000\nRRULE:FREQ=DAILY;COUNT=2\nSUMMARY:Class\nEND:VEVENT\nEND:VCALENDAR";
        let events = parse(ics, now, 0);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].end, Some(events[0].start + 3600));
        assert_eq!(events[1].end, Some(events[1].start + 3600));
    }

    #[test]
    fn parses_one_off_recurring_and_skips() {
        let now = days_from_civil(2026, 10, 19) * 86400 + 8 * 3600; // Monday 08:00
        let ev = parse(ICS, now, 0);
        let titles: Vec<_> = ev.iter().map(|e| (e.title.as_str(), (e.start - now) / 3600)).collect();
        // All-day events stay visible throughout the day, while cancelled
        // meetings and excluded recurring instances remain absent.
        // The 14-day lookahead also includes next week's valid recurrences.
        assert_eq!(titles, vec![("Holiday", -8), ("Sync", 1), ("Standup", 2), ("Sync", 169), ("Sync", 193)]);
        assert!(ev[0].all_day);
        assert_eq!(ev[2].link.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
        assert!(soon(&ev, now - 120).is_none());
    }

    #[test]
    fn utc_converts_to_local_and_soon_window() {
        let now = days_from_civil(2026, 10, 19) * 86400 + 11 * 3600; // 11:00 local
        let ev = parse(ICS, now, 3600); // local is UTC+1: Standup 10:00Z = 11:00 local
        assert_eq!(ev.iter().find(|e| e.title == "Standup").map(|e| e.start), Some(now));
        assert!(soon(&ev, now - 120).is_some_and(|e| e.title == "Standup"));
        assert!(soon(&ev, now - 600).is_none());
    }

    #[test]
    fn monthly_yearly_and_count() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:monthly\nDTSTART:20260907T100000\nRRULE:FREQ=MONTHLY;COUNT=3;BYDAY=1MO\nSUMMARY:First Monday\nEND:VEVENT\nBEGIN:VEVENT\nUID:annual\nDTSTART;VALUE=DATE:20251111\nRRULE:FREQ=YEARLY;COUNT=3\nSUMMARY:Annual leave\nEND:VEVENT\nEND:VCALENDAR";
        let now = days_from_civil(2026, 11, 7) * 86400;
        let events = parse(ics, now, 0);
        assert!(events.iter().any(|e| e.title == "First Monday" && e.start == days_from_civil(2026, 11, 2) * 86400 + 10 * 3600) == false);
        assert!(events.iter().any(|e| e.title == "Annual leave" && e.all_day && e.start == days_from_civil(2026, 11, 11) * 86400));
        assert!(!events.iter().any(|e| e.title == "First Monday" && e.start >= now));
    }

    #[test]
    fn monthly_first_monday_and_last_friday() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nDTSTART:20261005T100000\nRRULE:FREQ=MONTHLY;BYDAY=1MO\nSUMMARY:First Monday\nEND:VEVENT\nBEGIN:VEVENT\nUID:b\nDTSTART:20261030T120000\nRRULE:FREQ=MONTHLY;BYDAY=-1FR\nSUMMARY:Last Friday\nEND:VEVENT\nEND:VCALENDAR";
        let now = days_from_civil(2026, 11, 1) * 86400;
        let ev = parse(ics, now, 0);
        assert!(ev.iter().any(|e| e.title == "First Monday" && e.start == days_from_civil(2026, 11, 2) * 86400 + 10 * 3600));
        assert!(!ev.iter().any(|e| e.title == "Last Friday"));
    }

    #[test]
    fn weekly_count_limits_occurrences() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:weekly\nDTSTART:20261102T090000\nRRULE:FREQ=WEEKLY;COUNT=2;BYDAY=MO\nSUMMARY:Class\nEND:VEVENT\nEND:VCALENDAR";
        let now = days_from_civil(2026, 11, 7) * 86400;
        let ev = parse(ics, now, 0);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].start, days_from_civil(2026, 11, 9) * 86400 + 9 * 3600);
    }
}
