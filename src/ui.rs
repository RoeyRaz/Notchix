//! Immediate-mode drawing of the pill and the expanded panel. Every frame the
//! visible controls re-register themselves in `hits`, which `App` uses for
//! hover and click handling. All coordinates are logical (DPI-independent).

use crate::app::*;
use crate::gfx::*;
use crate::store::{clip_kind_of, preview, ClipKind};
use crate::sys;

const BLACK: Color = Color(0.0, 0.0, 0.0, 1.0);
const T1: Color = Color::white(0.97);
const T2: Color = Color::white(0.79);
const T3: Color = Color::white(0.56);
// Restrained, translucent surfaces inspired by the NotchPixel glass and
// graphite themes. They stay crisp with or without DWM glass support.
const SURF: Color = Color::white(0.075);
const HOVER: Color = Color::white(0.145);

// Neutral material; album art colours text and controls, not the surfaces.
fn glass_card(a: &App, r: Rect, radius: f32, ca: f32, hovered: bool) {
    let g = &a.gfx;
    g.fill_rrect(r, radius, Color(0.12, 0.13, 0.15, (if hovered { 0.58 } else { 0.46 }) * ca));
    g.fill_rrect(r, radius, Color::white((if hovered { 0.075 } else { 0.035 }) * ca));
    g.stroke_rrect(r, radius, Color(0.0, 0.0, 0.0, 0.16 * ca), 0.8);
}

// A calm purple when no cover is available; live artwork remains the accent.
// The clock is already read by the UI, so this adds no timer or wallpaper scan.
fn glass_accent(a: &App) -> Color {
    if a.art.is_some() { return a.accent; }
    let (_, _, _, _, hour, _, _, _) = sys::local_time();
    if (7..18).contains(&hour) { Color(0.75, 0.62, 0.87, 1.0) }
    else { Color(0.58, 0.50, 0.92, 1.0) }
}

/// How far the page area sits below the top of the open panel (the tab row is above it).
pub const CONTENT_Y: f32 = 70.0;

fn notification_tint(app: &str) -> Color {
    let name = app.to_ascii_lowercase();
    if name.contains("whatsapp") { Color(0.13, 0.82, 0.48, 1.0) }
    else if name.contains("discord") { Color(0.50, 0.53, 1.0, 1.0) }
    else if name.contains("telegram") { Color(0.28, 0.69, 0.98, 1.0) }
    else if name.contains("spotify") { Color(0.24, 0.85, 0.48, 1.0) }
    else if name.contains("teams") { Color(0.61, 0.52, 0.96, 1.0) }
    else if name.contains("slack") { Color(0.91, 0.58, 0.77, 1.0) }
    else if name.contains("chrome") || name.contains("edge") { Color(0.39, 0.76, 0.97, 1.0) }
    else if name.contains("calendar") { Color(0.98, 0.68, 0.41, 1.0) }
    else if name.contains("steam") || name.contains("game") { Color(0.44, 0.72, 0.98, 1.0) }
    else if name.contains("settings") || name.contains("system") { Color(0.67, 0.72, 0.82, 1.0) }
    else if name.contains("outlook") || name.contains("mail") { Color(0.38, 0.67, 1.0, 1.0) }
    else { Color(0.82, 0.77, 1.0, 1.0) }
}

fn notification_glyph(app: &str) -> char {
    let name = app.to_ascii_lowercase();
    if name.contains("whatsapp") || name.contains("telegram") || name.contains("discord") || name.contains("teams") { '\u{E8F2}' }
    else if name.contains("spotify") { '\u{E8D6}' }
    else if name.contains("calendar") { '\u{E787}' }
    else if name.contains("outlook") || name.contains("mail") { '\u{E715}' }
    else if name.contains("settings") { '\u{E713}' }
    else if name.contains("steam") || name.contains("game") { '\u{E7FC}' }
    else { '\u{E7F4}' }
}

fn draw_notice_icon(a: &App, name: &str, r: Rect, ca: f32) {
    let tint = notification_tint(name);
    a.gfx.fill_rrect(r, 11.0, tint.fade(0.14 * ca));
    let lower = name.to_ascii_lowercase();
    if lower.contains("whatsapp") {
        a.gfx.fill_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w.min(r.h) * 0.34, tint.fade(0.86 * ca));
        a.gfx.icon('\u{E717}', r, &a.gfx.fonts.icon14, Color::white(ca));
        return;
    }
    // Reuse genuine shortcut icons whenever the user's Apps list has the
    // corresponding application. The symbolic icon is the safe fallback.
    if let Some((i, _)) = a.apps.iter().enumerate().find(|(_, entry)| {
        entry.name.eq_ignore_ascii_case(name)
    }) {
        if let Some(Some(bmp)) = a.app_icons.get(i) {
            a.gfx.draw_bitmap(bmp, Rect::new(r.x + 3.0, r.y + 3.0, r.w - 6.0, r.h - 6.0), 7.0, ca);
            return;
        }
    }
    if lower.contains("discord") {
        a.gfx.fill_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w.min(r.h) * 0.34, tint.fade(0.86 * ca));
        a.gfx.text("D", r, &a.gfx.fonts.body, Color::white(ca), Align::Center);
        return;
    }
    a.gfx.icon(notification_glyph(name), r, &a.gfx.fonts.icon14, tint.fade(ca));
}

/// Travel only along the visible edge of a top-attached notch. The top is
/// clipped by the screen edge, so a rectangular perimeter would draw outside
/// the silhouette and cut across the contents.
fn rim_point(p: f32, w: f32, h: f32) -> (f32, f32) {
    let inset = 2.0;
    if p >= 1.0 { return (w - inset, 0.0); }
    let radius = (h * 0.43).min(21.0).min((w - 2.0 * inset) * 0.25);
    let side = (h - inset - radius).max(0.0);
    let arc = std::f32::consts::FRAC_PI_2 * radius;
    let bottom = (w - 2.0 * inset - 2.0 * radius).max(0.0);
    let length = 2.0 * side + 2.0 * arc + bottom;
    let mut d = p.clamp(0.0, 1.0) * length;
    if d <= side { return (inset, d); }
    d -= side;
    if d <= arc {
        let angle = std::f32::consts::PI - d / radius;
        return (inset + radius + radius * angle.cos(), h - inset - radius + radius * angle.sin());
    }
    d -= arc;
    if d <= bottom { return (inset + radius + d, h - inset); }
    d -= bottom;
    if d <= arc {
        let angle = std::f32::consts::FRAC_PI_2 - d / radius;
        return (w - inset - radius + radius * angle.cos(), h - inset - radius + radius * angle.sin());
    }
    d -= arc;
    (w - inset, (h - inset - radius - d).max(0.0))
}

#[cfg(test)]
mod rim_tests {
    use super::rim_point;

    #[test]
    fn progress_stays_on_attached_notch_edge() {
        for width in [180.0, 320.0, 500.0] {
            let height = 46.0;
            assert_eq!(rim_point(0.0, width, height), (2.0, 0.0));
            assert_eq!(rim_point(1.0, width, height), (width - 2.0, 0.0));
            for n in 0..=100 {
                let (x, y) = rim_point(n as f32 / 100.0, width, height);
                assert!((2.0..=width - 2.0).contains(&x));
                assert!((0.0..=height - 2.0).contains(&y));
            }
        }
    }
}

pub fn draw(a: &App, w: f32, h: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let t = a.t_expand();
    let rad = a.col_rad() + (36.0 - a.col_rad()) * t;

    // Move the upper rounded corners above the screen. Both collapsed and
    // expanded states attach to the top with soft lower corners.
    let beat = a.beat_factor();
    let inset = (a.squish * a.squish * 3.0).max(beat * 2.0);
    let body_top = -rad;
    let body = Rect::new(inset, body_top, w - 2.0 * inset, h - body_top - inset);
    // The material stays neutral across albums and changing wallpapers.
    let (_, _, _, _, hour, _, _, _) = sys::local_time();
    let day = (7..18).contains(&hour);
    let live = a.glass && a.live_glass.is_some() && a.glass_bitmap.is_some();
    let base = if a.glass {
        if live { Color(0.07, 0.075, 0.09, if day { 0.23 } else { 0.28 }) }
        else if day { Color(0.10, 0.10, 0.11, 0.67) } else { Color(0.07, 0.075, 0.085, 0.73) }
    } else { Color(0.042, 0.047, 0.050, 1.0) };
    if live {
        if let Some(bitmap) = &a.glass_bitmap {
            g.draw_bitmap_mapped(bitmap, Rect::new(0.0, 0.0, w, h), body, rad, 0.95);
        }
    }
    g.fill_rrect(body, rad, base);
    if a.glass { g.fill_rrect(body, rad, Color::white(if live { 0.035 } else { 0.025 })); }
    if a.glass && t < 0.05 && !a.mstate.has_track() && !a.alerting() {
        // A very small pool of reflected light gives the resting acrylic
        // depth without turning the entire edge into a bright outline.
        let accent = glass_accent(a);
        for i in 0..24 {
            let p = 0.38 + i as f32 * 0.24 / 24.0;
            let strength = (1.0 - ((i as f32 - 11.5) / 12.0).abs()).max(0.0);
            g.line(rim_point(p, w, h), rim_point(p + 0.24 / 24.0, w, h), 3.0,
                accent.fade(0.11 * strength));
        }
    }
    if a.glass && t > 0.1 {
        // Neutral smoked glass. Desktop colors and album art provide the
        // personality, not a permanent blue tint on every page.
        g.fill_rrect(Rect::new(8.0, -rad + 4.0, w - 16.0, h + rad - 12.0), (rad - 2.0).max(4.0), Color::white(0.045 * t));
    }
    g.stroke_rrect(
        Rect::new(body.x + 0.5, body.y + 0.5, body.w - 1.0, body.h - 1.0),
        rad,
        Color(0.0, 0.0, 0.0, 0.22),
        0.8,
    );
    // Keep the resting rim neutral. Accent is reserved for artwork, actual
    // notifications and interaction, never fixed pink/green edge segments.
    if !a.expanded && t < 0.05 {
        if let Some((notice, _)) = &a.system_preview {
            let elapsed = notice.received.elapsed().as_secs_f32();
            let tint = notification_tint(&notice.app);
            let entrance = (elapsed / 0.30).clamp(0.0, 1.0);
            let fade = (1.0 - ((elapsed - 0.55) / 1.65).max(0.0)).clamp(0.0, 1.0);
            let intensity = entrance * fade;
            // App-colored glass catches the light, while the dark center
            // keeps notification text legible against any wallpaper.
            g.fill_rrect(body, rad, tint.fade(0.035 * intensity));
            g.stroke_rrect(Rect::new(body.x + 1.5, body.y + 1.0, body.w - 3.0, body.h - 2.0), rad,
                tint.fade(0.13 + 0.38 * intensity), 1.25);
            // The highlight traverses the rim only once. A soft tail creates
            // motion without continuously flashing around the monitor.
            if elapsed < 2.2 {
                let head = ((elapsed - 0.10) / 1.65).clamp(0.0, 1.0);
                for i in 0..12 {
                    let offset = i as f32 * 0.004;
                    let p0 = rim_point((head - offset).max(0.0), w, h);
                    let p1 = rim_point((head - offset - 0.003).max(0.0), w, h);
                    let tail = 1.0 - i as f32 / 12.0;
                    g.line(p0, p1, 2.0 + 1.2 * tail, tint.fade(intensity * 0.75 * tail));
                }
            }
        }
    }
    if beat > 0.01 {
        g.stroke_rrect(body, rad, Color(1.0, 0.4, 0.5, 0.85 * beat), 2.5);
    }
    if a.level > 0.01 && t < 0.05 && a.mstate.has_track() {
        // A small coloured rim reacts to music without tinting the shell.
        g.stroke_rrect(body, rad, a.accent.fade(0.08 + 0.14 * a.level), 0.85);
    }

    if a.alerting() && t < 0.05 {
        let pulse = 0.5 + 0.5 * (a.eq_phase * 0.9).sin();
        // important hints get their own strong colour so they read from the corner of the eye
        let (tint, base) = match a.alert_msg {
            "Meeting" => (Color(0.20, 0.45, 1.0, 1.0), 0.55),
            "Battery low" => (Color(1.0, 0.50, 0.10, 1.0), 0.55),
            "Take a break" => (Color(0.12, 0.72, 0.62, 1.0), 0.50),
            _ => (a.accent, 0.25),
        };
        g.fill_rrect(Rect::new(0.0, -rad, w, h + rad), rad, tint.fade(base + (0.95 - base) * 0.5 * pulse));
    }
    let c_alpha = (1.0 - t * 3.2).clamp(0.0, 1.0);
    // Wait until the growing shell has room for the panel controls. Height
    // and width spring independently, especially from a long media pill.
    let width_ready = ((w / EXP_W - 0.78) / 0.22).clamp(0.0, 1.0);
    let e_alpha = ((t - 0.48) / 0.52).clamp(0.0, 1.0).min(width_ready);
    if c_alpha > 0.0 {
        draw_collapsed(a, w, c_alpha);
        draw_privacy_indicators(a, w, c_alpha);
        draw_idle_progress(a, w, h, c_alpha);
    }
    if e_alpha > 0.0 {
        draw_expanded(a, e_alpha, hits);
    }
    if let Some(ct) = a.confetti_t() {
        draw_confetti(a, w, h, ct);
    }
}

fn draw_idle_progress(a: &App, w: f32, h: f32, alpha: f32) {
    if a.expanded || a.system_preview.is_some() { return; }
    let now = crate::cal::now().0;
    let progress = crate::cal::EVENTS.lock().ok().and_then(|events|
        outline_progress(&events, now, &a.mstate, sys::filetime_now(), a.cfg.day_line));
    let Some(progress) = progress else { return };
    let g = &a.gfx;
    let color = glass_accent(a).fade(0.82 * alpha);
    let steps = 96;
    let visible = (progress.clamp(0.0, 1.0) * steps as f32).ceil() as usize;
    for i in 0..visible {
        let p0 = rim_point(i as f32 / steps as f32, w, h);
        let p1 = rim_point(((i + 1) as f32 / steps as f32).min(progress), w, h);
        g.line(p0, p1, 2.4, color);
    }
}

fn outline_progress(events: &[crate::cal::Event], now: i64, media: &crate::media::MediaState, now_ft: i64, song_enabled: bool) -> Option<f32> {
    if let Some((start, end)) = events.iter().filter_map(|event| {
        let end = event.end?;
        (!event.all_day && event.start <= now && end > now && end > event.start).then_some((event.start, end))
    }).min_by_key(|(start, _)| *start) {
        return Some((now - start) as f32 / (end - start) as f32);
    }
    if song_enabled && media.has_track() { song_progress(media, now_ft) } else { None }
}

fn song_progress(media: &crate::media::MediaState, now_ft: i64) -> Option<f32> {
    let (position, duration) = media.progress(now_ft);
    if duration > 0.0 && position.is_finite() { Some((position / duration).clamp(0.0, 1.0) as f32) } else { None }
}

#[cfg(test)]
mod song_progress_tests {
    use super::{outline_progress, song_progress};
    use crate::cal::Event;
    use crate::media::MediaState;

    #[test]
    fn uses_real_media_timeline_only() {
        let mut media = MediaState::default();
        assert_eq!(song_progress(&media, 0), None);
        media.dur_ticks = 100 * 10_000_000;
        media.pos_ticks = 25 * 10_000_000;
        assert_eq!(song_progress(&media, 0), Some(0.25));
        media.pos_ticks = 200 * 10_000_000;
        assert_eq!(song_progress(&media, 0), Some(1.0));
    }

    #[test]
    fn timed_event_takes_priority_over_song() {
        let mut media = MediaState::default();
        media.has_session = true;
        media.title = "Song".into();
        media.dur_ticks = 100 * 10_000_000;
        media.pos_ticks = 25 * 10_000_000;
        let event = Event { start: 100, end: Some(200), title: "Meeting".into(), link: None, all_day: false };
        assert_eq!(outline_progress(&[event], 150, &media, 0, true), Some(0.5));
        assert_eq!(outline_progress(&[], 150, &media, 0, true), Some(0.25));
        assert_eq!(outline_progress(&[], 150, &media, 0, false), None);
    }
}

/// Tiny Windows-style capture lights, independent of the music/download hierarchy.
/// Both can be visible at once without taking over the island.
fn draw_privacy_indicators(a: &App, w: f32, alpha: f32) {
    let mic = a.call_since.is_some();
    let cam = a.camera_active;
    if !mic && !cam { return; }
    let g = &a.gfx;
    let y = 6.0;
    if cam {
        g.fill_circle(w - 9.0, y, 4.4, Color::white(0.18 * alpha));
        g.fill_circle(w - 9.0, y, 2.9, Color(0.24, 0.9, 0.52, alpha));
    }
    if mic {
        let x = if cam { w - 21.0 } else { w - 9.0 };
        g.fill_circle(x, y, 4.4, Color::white(0.18 * alpha));
        g.fill_circle(x, y, 2.9, Color(1.0, 0.65, 0.28, alpha));
    }
}
/// Confetti burst from the pill (or panel) after the focus timer ends: pure
/// functions of time, so there is no particle state to keep.
fn draw_confetti(a: &App, w: f32, h: f32, t: f32) {
    let g = &a.gfx;
    const COLORS: [Color; 6] = [
        Color(1.0, 0.45, 0.5, 1.0),
        Color(1.0, 0.8, 0.3, 1.0),
        Color(0.5, 0.9, 0.6, 1.0),
        Color(0.45, 0.75, 1.0, 1.0),
        Color(0.8, 0.6, 1.0, 1.0),
        Color(1.0, 1.0, 1.0, 1.0),
    ];
    let fade = (1.0 - (t - 2.4).max(0.0) / 0.6).clamp(0.0, 1.0);
    for i in 0..40u32 {
        let r = |k: u32| {
            let mut x = (i + 1).wrapping_mul(2654435761).wrapping_add(k.wrapping_mul(40503));
            x ^= x >> 15;
            x = x.wrapping_mul(2246822519);
            x ^= x >> 13;
            (x % 1000) as f32 / 1000.0
        };
        let x = w * (0.2 + 0.6 * r(1)) + (r(2) - 0.5) * 150.0 * t;
        let y = h * 0.5 - (30.0 + 80.0 * r(3)) * t + 0.5 * 220.0 * t * t;
        if y > h + 6.0 || x < -4.0 || x > w + 4.0 {
            continue;
        }
        let spin = (t * (6.0 + 8.0 * r(4)) + r(5) * 6.0).sin().abs();
        let c = COLORS[(r(6) * 6.0) as usize % 6];
        g.fill_rrect(Rect::new(x, y, 2.0 + 3.0 * spin, 4.0 - 2.0 * spin), 0.5, c.fade(fade));
    }
}


// ------------------------------------------------------------- collapsed ----

fn draw_collapsed(a: &App, w: f32, ca: f32) {
    let g = &a.gfx;
    let f = &g.fonts;
    let ch = a.col_h();

    if a.alerting() {
        let white = Color(1.0, 1.0, 1.0, ca);
        let amber = Color(1.0, 0.72, 0.2, ca);
        if a.alert_msg == "Volume" || a.alert_msg == "Brightness" {
            let level = a.osd_level.unwrap_or(0).min(100);
            let bright = a.alert_msg == "Brightness";
            g.icon(if bright { '\u{E706}' } else if a.osd_muted || level == 0 { '\u{E74F}' } else { '\u{E767}' }, Rect::new(13.0, 0.0, 24.0, ch), &f.icon14, white);
            let track = Rect::new(44.0, ch / 2.0 - 3.0, w - 103.0, 6.0);
            g.fill_rrect(track, 3.0, Color::white(0.22 * ca));
            g.fill_rrect(Rect::new(track.x, track.y, track.w * if !bright && a.osd_muted { 0.0 } else { level as f32 / 100.0 }, track.h), 3.0, a.accent.fade(ca));
            g.text(if !bright && a.osd_muted { "Mute".to_string() } else { format!("{level}%") }.as_str(), Rect::new(w - 49.0, 0.0, 43.0, ch), &f.small, white, Align::Right);
            return;
        }
        let mut label = match (a.alert_msg, a.alert_start) {
            ("Meeting", Some(start)) if !a.alert_label.is_empty() => {
                let mins = (start - crate::cal::now().0 + 59).div_euclid(60);
                format!("{} {}", a.alert_label, if mins > 0 { format!("{mins}m") } else { "now".to_string() })
            }
            _ => a.alert_msg.to_string(),
        };
        let mut col = white;
        if let Some(sw) = a.alert_swatch {
            let (cx, cy) = (w / 2.0 - 52.0, ch / 2.0);
            g.fill_circle(cx, cy, 7.0, Color::white(0.35 * ca));
            g.fill_circle(cx, cy, 6.0, Color(sw.0, sw.1, sw.2, ca));
        } else if a.alert_msg == "Battery low" {
            col = amber;
            if let Some(s) = sys::battery_info().filter(|b| !b.plugged).and_then(|b| b.secs_left) {
                label = format!("Battery low {}", if s >= 3600 { format!("{}h{:02}m", s / 3600, s % 3600 / 60) } else { format!("{}m", s / 60) });
            }
            g.icon('\u{E850}', Rect::new(w / 2.0 - 66.0, 0.0, 20.0, ch), &f.icon14, amber);
        } else if a.alert_msg == "Meeting" {
            g.icon('\u{E787}', Rect::new(16.0, 0.0, 20.0, ch), &f.icon14, white);
        } else if a.alert_msg == "Look away" {
            // a drawn eye
            let (ex, ey) = (w / 2.0 - 50.0, ch / 2.0);
            g.fill_rrect(Rect::new(ex - 8.0, ey - 5.0, 16.0, 10.0), 5.0, Color(1.0, 1.0, 1.0, ca));
            g.fill_circle(ex, ey, 3.4, Color(0.25, 0.55, 0.9, ca));
            g.fill_circle(ex, ey, 1.5, Color(0.0, 0.0, 0.0, ca));
        } else if a.alert_msg == "Drink water" {
            // a drawn water drop
            let (dx, dy) = (w / 2.0 - 52.0, ch / 2.0 + 1.0);
            g.fill_circle(dx, dy + 2.0, 5.0, Color(0.45, 0.8, 1.0, ca));
            g.fill_poly(&[(dx - 4.4, dy), (dx + 4.4, dy), (dx, dy - 8.0)], Color(0.45, 0.8, 1.0, ca));
        } else {
            g.icon('\u{E916}', Rect::new(w / 2.0 - 58.0, 0.0, 20.0, ch), &f.icon14, white);
        }
        let tx = if a.alert_msg == "Meeting" { 44.0 } else if a.alert_msg == "Battery low" { w / 2.0 - 42.0 } else { w / 2.0 - 34.0 };
        g.text(&label, Rect::new(tx, 0.0, (w - tx - 14.0).max(118.0), ch), &f.body, col, Align::Left);
        return;
    }

    if let Some((notice, until)) = &a.system_preview {
        let source = crate::win_notifications::display_source(&notice.app, &notice.title, &notice.body);
        let kind = crate::win_notifications::classify(&notice.app, &notice.title, &notice.body);
        let shade = notification_tint(source).fade(ca);
        let elapsed = notice.received.elapsed().as_secs_f32();
        let remaining = until.saturating_duration_since(std::time::Instant::now()).as_secs_f32();
        // Content follows the spring shape. A soft entrance and exit keep the
        // notification feeling attached to the island instead of a toast.
        let easing = |v: f32| { let v = v.clamp(0.0, 1.0); v * v * (3.0 - 2.0 * v) };
        let appear = easing(elapsed / 0.34);
        let vanish = easing(remaining / 0.48);
        let opacity = ca * appear.min(vanish);
        let slide = (1.0 - appear) * 4.0;
        let icon = Rect::new(11.0, 15.0 + slide, 38.0, 38.0);
        if a.mascot_visible {
            // Miso reacts to the incoming message rather than disappearing.
            g.fill_rrect(icon, 15.0, shade.fade(0.13 * opacity));
            draw_miso_at(a, opacity, 29.0, 34.0, true);
            g.fill_circle(43.0, 47.0 + slide, 4.2, shade.fade(opacity));
        } else {
            draw_notice_icon(a, source, icon, opacity);
        }
        if kind != crate::win_notifications::NoticeKind::Regular && !a.private_notifications {
            g.fill_rrect(Rect::new(55.0, 17.0 + slide, 2.0, 37.0), 1.0, shade.fade(opacity * 0.8));
            let badge = Rect::new(239.0, 9.0 + slide, 111.0, 17.0);
            g.fill_rrect(badge, 8.5, shade.fade(opacity * 0.15));
            g.text(kind.badge(), badge, &f.tiny, shade.fade(opacity), Align::Center);
        }
        g.text(source, Rect::new(60.0, 10.0 + slide, 172.0, 17.0), &f.tiny, shade.fade(opacity), Align::Left);
        let title = if a.private_notifications { "Notification" } else { &notice.title };
        g.text(title, Rect::new(60.0, 28.0 + slide, 294.0, 20.0), &f.body, T1.fade(opacity), Align::Left);
        // A single-line body provides context without turning this into a
        // three-row banner. The complete notification is in Activity.
        if !notice.body.is_empty() {
            let body = if a.private_notifications { "Content hidden" } else { &notice.body };
            g.text(body, Rect::new(60.0, 48.0 + slide, 294.0, 14.0), &f.tiny, T2.fade(opacity * 0.9), Align::Left);
        }
        // An understated lifetime indicator avoids a static banner feeling.
        let life = (remaining / kind.preview_seconds() as f32).clamp(0.0, 1.0);
        g.fill_rrect(Rect::new(60.0, 63.0, 288.0 * life, 1.2), 0.6, shade.fade(opacity * 0.48));
        return;
    }

    let copy = crate::explorer_copy::current();
    let qbit = crate::qbit::current();
    let prism = crate::prism::current();
    let transfer = crate::transfer::current();
    // A transfer is a secondary live activity whenever music is present.
    // Never force the user to choose between Now Playing and downloads.
    if a.mstate.has_track() && (copy.is_some() || qbit.is_some() || prism.is_some() || transfer.is_some()) {
        let inset = if a.mascot_visible { 40.0 } else { 0.0 };
        if a.mascot_visible { draw_miso(a, ca); }
        g.shifted(inset, 0.0, || draw_collapsed_regular(a, w - inset - 78.0, ca));
        g.fill_rrect(Rect::new(w - 78.0, ch / 2.0 - 13.0, 1.0, 26.0), 0.5, Color::white(0.16 * ca));
        let (progress, label, count) = if let Some(progress) = copy {
            (Some(progress), "Files".to_string(), 1 + usize::from(qbit.is_some()) + prism.as_ref().map(|p| p.count).unwrap_or(0) + usize::from(transfer.is_some()))
        } else if let Some(item) = qbit.as_ref() {
            let eta = item.eta.map(|s| if s >= 3600 { format!("{}h{:02}m", s / 3600, s % 3600 / 60) } else { format!("{}m", (s + 59) / 60) }).unwrap_or_else(|| "qBit".into());
            (Some(item.percent), eta, item.count + prism.as_ref().map(|p| p.count).unwrap_or(0) + usize::from(transfer.is_some()))
        } else if let Some(item) = prism.as_ref() {
            let eta = item.eta.map(|s| if s >= 3600 { format!("{}h{:02}m", s / 3600, s % 3600 / 60) } else { format!("{}m", (s + 59) / 60) }).unwrap_or_else(|| "Prism".into());
            (item.percent, eta, item.count + usize::from(transfer.is_some()))
        } else {
            (None, transfer.as_ref().map(|v| v.app.clone()).unwrap_or_default(), 1)
        };
        draw_transfer_ring(a, w - 55.0, ch / 2.0, ca, progress);
        let short = if label.chars().count() > 10 { format!("{}…", label.chars().take(9).collect::<String>()) } else { label };
        g.text(&short, Rect::new(w - 37.0, ch / 2.0 - 12.0, 34.0, 24.0), &f.tiny, T2.fade(ca), Align::Center);
        if count > 1 {
            g.fill_circle(w - 40.0, ch / 2.0 - 12.0, 7.0, a.accent.fade(ca));
            g.text(&format!("{count}"), Rect::new(w - 47.0, ch / 2.0 - 19.0, 14.0, 14.0), &f.tiny, Color::white(ca), Align::Center);
        }
        return;
    }

    if let Some(progress) = copy {
        let left = if a.mascot_visible { 52.0 } else { 17.0 };
        if a.mascot_visible { draw_miso(a, ca); }
        let accent = a.accent.fade(ca);
        g.icon('\u{E896}', Rect::new(left, 0.0, 22.0, ch), &f.icon14, accent);
        g.text("Explorer file operation", Rect::new(left + 28.0, 2.0, (w - left - 42.0).max(40.0), (ch / 2.0).max(17.0)), &f.tiny, T2.fade(ca), Align::Left);
        g.text(&format!("{}% · Windows progress", (progress * 100.0).round() as u32), Rect::new(left + 28.0, (ch / 2.0 - 1.0).max(15.0), (w - left - 42.0).max(40.0), (ch / 2.0).max(17.0)), &f.small, T1.fade(ca), Align::Left);
        let track = Rect::new(left + 28.0, ch - 7.0, (w - left - 50.0).max(30.0), 4.0);
        g.fill_rrect(track, 2.0, Color::white(0.19 * ca));
        g.fill_rrect(Rect::new(track.x, track.y, track.w * progress, track.h), 2.0, accent);
        return;
    }

    if let Some(item) = qbit {
        let left = if a.mascot_visible { 52.0 } else { 17.0 };
        if a.mascot_visible { draw_miso(a, ca); }
        let accent = a.accent.fade(ca);
        g.icon('\u{E896}', Rect::new(left, 0.0, 22.0, ch), &f.icon14, accent);
        let eta = item.eta.map(|s| if s >= 3600 { format!("{}h {}m", s / 3600, (s % 3600) / 60) } else { format!("{}m", (s + 59) / 60) }).unwrap_or_else(|| "ETA unknown".into());
        let detail = format!("{}%  ·  {}  ·  {}", (item.percent * 100.0).round() as u32, crate::transfer::rate_label(item.speed), eta);
        g.text(&format!("qBittorrent · {}", item.name), Rect::new(left + 28.0, 2.0, (w - left - 42.0).max(40.0), (ch / 2.0).max(17.0)), &f.tiny, T2.fade(ca), Align::Left);
        g.text(&detail, Rect::new(left + 28.0, (ch / 2.0 - 1.0).max(15.0), (w - left - 42.0).max(40.0), (ch / 2.0).max(17.0)), &f.small, T1.fade(ca), Align::Left);
        let track = Rect::new(left + 28.0, ch - 7.0, (w - left - 50.0).max(30.0), 4.0);
        g.fill_rrect(track, 2.0, Color::white(0.19 * ca));
        g.fill_rrect(Rect::new(track.x, track.y, track.w * item.percent, track.h), 2.0, accent);
        return;
    }

    if let Some(item) = prism {
        let left = if a.mascot_visible { 52.0 } else { 17.0 };
        if a.mascot_visible { draw_miso(a, ca); }
        let accent = a.accent.fade(ca);
        g.icon('\u{E896}', Rect::new(left, 0.0, 22.0, ch), &f.icon14, accent);
        let pct = item.percent.map(|p| format!("{}%", (p * 100.0).round() as u32)).unwrap_or_else(|| item.state.clone());
        let eta = item.eta.map(|s| if s >= 3600 { format!("{}h {}m", s / 3600, (s % 3600) / 60) } else { format!("{}m", (s + 59) / 60) }).unwrap_or_else(|| "ETA unknown".into());
        let detail = format!("{}  ·  {}  ·  {}", pct, crate::transfer::rate_label(item.speed), eta);
        g.text(&format!("Prism · {}", item.name), Rect::new(left + 28.0, 2.0, (w - left - 42.0).max(40.0), (ch / 2.0).max(17.0)), &f.tiny, T2.fade(ca), Align::Left);
        g.text(&detail, Rect::new(left + 28.0, (ch / 2.0 - 1.0).max(15.0), (w - left - 42.0).max(40.0), (ch / 2.0).max(17.0)), &f.small, T1.fade(ca), Align::Left);
        let track = Rect::new(left + 28.0, ch - 7.0, (w - left - 50.0).max(30.0), 4.0);
        g.fill_rrect(track, 2.0, Color::white(0.19 * ca));
        if let Some(progress) = item.percent {
            g.fill_rrect(Rect::new(track.x, track.y, track.w * progress, track.h), 2.0, accent);
        } else {
            let phase = (sys::filetime_now().rem_euclid(20_000_000) as f32) / 20_000_000.0;
            g.fill_rrect(Rect::new(track.x + track.w * phase * 0.70, track.y, track.w * 0.30, track.h), 2.0, accent);
        }
        return;
    }

    if let Some(activity) = transfer {
        let left = if a.mascot_visible { 52.0 } else { 17.0 };
        if a.mascot_visible { draw_miso(a, ca); }
        let accent = a.accent.fade(ca);
        g.icon('\u{E896}', Rect::new(left, 0.0, 22.0, ch), &f.icon14, accent);
        let label = if activity.app == "Browser download" {
            format!("Browser download · {}", crate::transfer::rate_label(activity.bytes_per_second))
        } else {
            format!("{} active · {} I/O", activity.app, crate::transfer::rate_label(activity.bytes_per_second))
        };
        g.text(&label, Rect::new(left + 28.0, 0.0, (w - left - 42.0).max(40.0), ch - 9.0), &f.small, T1.fade(ca), Align::Left);
        // Moving activity strip: indeterminate on purpose. Process disk IO
        // cannot tell us the download's total size or actual percentage.
        let track = Rect::new(left + 28.0, ch - 7.0, (w - left - 50.0).max(30.0), 4.0);
        g.fill_rrect(track, 2.0, Color::white(0.19 * ca));
        let phase = (sys::filetime_now().rem_euclid(20_000_000) as f32) / 20_000_000.0;
        g.fill_rrect(Rect::new(track.x + track.w * phase * 0.70, track.y, track.w * 0.30, track.h), 2.0, accent);
        return;
    }

    if a.mascot_visible {
        // The companion owns a tiny strip of real layout space. Music artwork,
        // skip controls, clock and focus timer retain their original widths.
        draw_miso(a, ca);
        g.shifted(40.0, 0.0, || draw_collapsed_regular(a, w - 40.0, ca));
    } else {
        draw_collapsed_regular(a, w, ca);
    }
}

/// Actual percentage for qBittorrent, moving dots for unquantified activity.
/// The ring is intentionally small so a download never obscures Now Playing.
fn draw_transfer_ring(a: &App, cx: f32, cy: f32, alpha: f32, progress: Option<f32>) {
    let g = &a.gfx;
    let segments = 36;
    let phase = sys::filetime_now().rem_euclid(20_000_000) as f32 / 20_000_000.0;
    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let x = cx + 11.0 * angle.cos();
        let y = cy + 11.0 * angle.sin();
        let active = progress.map_or_else(
            || ((i as f32 / segments as f32 - phase).rem_euclid(1.0)) < 0.24,
            |p| (i as f32 / segments as f32) < p.clamp(0.0, 1.0),
        );
        g.fill_circle(x, y, 1.35, if active { a.accent.fade(alpha) } else { Color::white(0.16 * alpha) });
    }
    if let Some(p) = progress {
        g.text(&format!("{}", (p.clamp(0.0, 1.0) * 100.0).round() as u32), Rect::new(cx - 11.0, cy - 8.0, 22.0, 16.0), &g.fonts.tiny, T1.fade(alpha), Align::Center);
    } else {
        g.icon('\u{E896}', Rect::new(cx - 7.0, cy - 7.0, 14.0, 14.0), &g.fonts.icon14, T2.fade(alpha));
    }
}

/// The normal collapsed content, deliberately unaware of the mascot.
fn draw_collapsed_regular(a: &App, w: f32, ca: f32) {
    let g = &a.gfx;
    let f = &g.fonts;
    let m = &a.mstate;
    let ch = a.col_h();
    let timer = a.timer_running();

    // Microphone activity does not replace the collapsed clock or media.
    // Nock can still monitor mic use and show it in other UI surfaces.

    if m.has_track() {
        let ar = Rect::new(8.0, (ch - 30.0) / 2.0, 30.0, 30.0);
        match &a.art {
            Some(bmp) => g.draw_bitmap(bmp, ar, 6.0, ca),
            None => {
                g.fill_rrect(ar, 6.0, Color::white(0.14 * ca));
                g.text("\u{266A}", ar, &f.small, T2.fade(ca), Align::Center);
            }
        }
        // Keep the time visible alongside Spotify, including when the mic is active.
        let (_, _, _, _, hh, mm, _, _) = sys::local_time();
        let clock = if a.cfg.clock_24h {
            format!("{hh:02}:{mm:02}")
        } else {
            format!("{}:{mm:02}", if hh % 12 == 0 { 12 } else { hh % 12 })
        };
        let clock_x = w - 75.0;
        g.fill_rrect(Rect::new(clock_x - 8.0, ch / 2.0 - 7.0, 1.0, 14.0), 0.5, Color::white(0.18 * ca));
        g.text(&clock, Rect::new(clock_x, 0.0, 70.0, ch), &f.body, T1.fade(ca), Align::Left);
        // Equalizer sits between the track information and the clock.
        let bx = clock_x - 36.0;
        draw_bars(a, bx, ch / 2.0, ca);
        let mut right = bx - 8.0;
        if timer {
            let r = Rect::new(right - 50.0, 0.0, 50.0, ch);
            g.text(
                &fmt_time(a.timer_remaining_ms() as f64 / 1000.0),
                r,
                &f.small,
                T1.fade(ca),
                Align::Right,
            );
            right -= 58.0;
        }
        let title = if m.title.is_empty() {
            "Unknown"
        } else {
            &m.title
        };
        let label = if m.artist.is_empty() {
            title.to_string()
        } else {
            format!("{}  \u{00B7}  {}", title, m.artist)
        };
        g.text(
            &label,
            Rect::new(47.0, 0.0, (right - 47.0 - 5.0).max(10.0), ch),
            &f.body,
            T1.fade(ca),
            Align::Center,
        );
    } else if timer {
        g.icon('\u{E916}', Rect::new(w / 2.0 - 36.0, 0.0, 18.0, ch), &f.icon14, T1.fade(ca));
        g.text(
            &fmt_time(a.timer_remaining_ms() as f64 / 1000.0),
            Rect::new(w / 2.0 - 14.0, 0.0, 56.0, ch),
            &f.small,
            T1.fade(ca),
            Align::Left,
        );
    } else {
        let (_, _, _, _, hh, mm, _, _) = sys::local_time();
        let label = if a.cfg.clock_24h {
            format!("{:02}:{:02}", hh, mm)
        } else {
            let (h12, ampm) = match hh {
                0 => (12, "AM"),
                1..=11 => (hh, "AM"),
                12 => (12, "PM"),
                _ => (hh - 12, "PM"),
            };
            format!("{}:{:02} {}", h12, mm, ampm)
        };
        let (year, month, day, _, ..) = sys::local_time();
        // date | time, centred as a pair; the time is the bold part
        let (dw, tw) = (76.0, if a.cfg.clock_24h { 44.0 } else { 70.0 });
        let glance = a.glance();
        let gw = glance.as_ref().map_or(0.0, |(_, gw)| gw + 16.0);
        let x0 = (w - (dw + 15.0 + tw + gw)) / 2.0;
        g.text(&format!("{day:02}/{month:02}/{:02}", year.rem_euclid(100)), Rect::new(x0, 0.0, dw, ch), &f.tiny, T3.fade(ca), Align::Right);
        g.fill_rrect(Rect::new(x0 + dw + 7.0, ch / 2.0 - 7.0, 1.0, 14.0), 0.5, Color::white(0.18 * ca));
        g.text(&label, Rect::new(x0 + dw + 15.0, 0.0, tw, ch), &f.menu, T1.fade(ca), Align::Left);
        if let Some((txt, gwid)) = glance {
            // optional glance: separator, then secondary-colour text (tasks get a small checkbox)
            let sx = x0 + dw + 15.0 + tw + 7.0;
            g.fill_rrect(Rect::new(sx, ch / 2.0 - 7.0, 1.0, 14.0), 0.5, Color::white(0.18 * ca));
            let mut gx = sx + 9.0;
            let kind = a.glance_kind();
            if kind == 1 {
                g.icon('\u{E73A}', Rect::new(gx, 0.0, 16.0, ch), &f.icon14, T2.fade(ca));
                gx += 19.0;
            }
            // the next meeting is dimmer than the other glances
            let gcol = if kind == 4 { Color(0.55, 0.53, 0.49, ca) } else { T2.fade(ca) };
            g.text(&txt, Rect::new(gx, 0.0, gwid, ch), &f.small, gcol, Align::Left);
        }
        if a.cycle_dots() {
            // page dots while cycling: the active page in blue, the others grey
            let active = a.cycle as usize % 4;
            for i in 0..4 {
                let c = if i == active { Color(DEFAULT_ACCENT.0, DEFAULT_ACCENT.1, DEFAULT_ACCENT.2, ca) } else { Color(0.267, 0.267, 0.267, ca) };
                g.fill_circle(w / 2.0 - 12.0 + i as f32 * 8.0, ch - 4.0, 2.0, c);
            }
        }
    }

    if timer && a.cfg.idle_focus_bar {
        draw_focus_line(a, w, ch, ca);
    }
    if a.cfg.state_dot && !m.has_track() {
        draw_state_dot(a, ch, ca);
    }
    if let Some(k) = a.peek_k() {
        use crate::buddy::BuddyKind;
        if let Some(kind @ (BuddyKind::Robot | BuddyKind::Cat)) = BuddyKind::from_id(a.cfg.buddy) {
            crate::buddy::draw_peek(g, kind, w - 26.0, ch, k, a.peek_t());
        }
    }
}

/// The notch itself is the companion. Just two living, luminous eyes embedded
/// in the glass: restrained while idle, playful with music, alert on hover.
/// Nothing sits on top of the time, album art or transport controls.
fn draw_miso(a: &App, ca: f32) {
    draw_miso_at(a, ca, 20.0, a.col_h() / 2.0, false);
}

fn draw_miso_at(a: &App, ca: f32, cx: f32, baseline: f32, reacting: bool) {
    let g = &a.gfx;
    let t = a.mascot_birth.elapsed().as_secs_f32();
    let playing = a.mstate.playing;
    let hovering = a.mascot_cursor().is_some_and(|x| x < 43.0);
    let playful = a.mascot_reaction_until.is_some_and(|until| std::time::Instant::now() < until);
    let cy = baseline + if reacting || playful { (t * 7.0).sin() * 1.2 } else if playing { (t * 4.2).sin() * 0.7 } else { (t * 0.7).sin() * 0.2 };
    let pearl = Color(0.96, 0.96, 1.0, ca);
    let blue = glass_accent(a).fade(ca);
    let glow = 0.06 + if reacting || playful { 0.15 } else if hovering { 0.11 } else if playing { 0.08 } else { 0.02 };
    g.fill_rrect(Rect::new(cx - 15.0, cy - 13.0, 30.0, 26.0), 13.0, blue.fade(glow));
    // Miso has a small soft shell and two rounded ears. Geometry is drawn in
    // the same reserved 40px area, with no image asset or extra animation.
    g.fill_circle(cx - 10.5, cy - 10.5, 4.0, Color(0.13, 0.14, 0.18, ca));
    g.fill_circle(cx + 10.5, cy - 10.5, 4.0, Color(0.13, 0.14, 0.18, ca));
    g.fill_rrect(Rect::new(cx - 14.0, cy - 11.0, 28.0, 24.0), 11.0, Color(0.11, 0.12, 0.16, ca));
    g.stroke_rrect(Rect::new(cx - 13.6, cy - 10.6, 27.2, 23.2), 10.6, blue.fade(ca * 0.32), 0.8);
    // Natural short double-blink at irregular-looking intervals.
    let beat = (t + 1.3).rem_euclid(5.7);
    let blink = if beat > 5.48 { (1.0 - ((beat - 5.48) / 0.22 * std::f32::consts::PI).sin()).clamp(0.06, 1.0) }
        else if beat > 5.12 && beat < 5.30 { (1.0 - ((beat - 5.12) / 0.18 * std::f32::consts::PI).sin()).clamp(0.06, 1.0) }
        else { 1.0 };
    let look = if reacting || playful { (t * 2.0).sin() * 0.2 } else if hovering {
        ((a.mascot_cursor().unwrap_or(cx) - cx) / 62.0).clamp(-1.0, 1.0)
    } else if playing { 0.3 } else { (t * 0.6).sin() * 0.58 };
    for dx in [-5.5f32, 5.5f32] {
        let ex = cx + dx + look * 1.2;
        let eye_h = if reacting { 9.3 } else if hovering { 9.0 } else { 8.2 };
        if (hovering || playing || reacting || playful) && blink > 0.5 {
            // A cheerful crescent eye echoes the expressive mascot in the
            // floating-glass concept. It opens back up while resting.
            g.line((ex - 3.2, cy + 0.7), (ex - 1.7, cy - 1.5), 1.8, pearl);
            g.line((ex - 1.7, cy - 1.5), (ex + 0.4, cy - 2.2), 1.8, pearl);
            g.line((ex + 0.4, cy - 2.2), (ex + 3.2, cy + 0.7), 1.8, pearl);
        } else {
            g.fill_rrect(Rect::new(ex - 2.25, cy - eye_h * 0.5 * blink, 4.5, eye_h * blink), 2.1, pearl);
            if blink > 0.55 { g.fill_circle(ex + 0.65, cy - 1.7, 0.65, Color(1.0, 1.0, 1.0, ca)); }
        }
    }
    // Expressions react to notifications, hovering, music and focus time.
    if reacting || playful || hovering || playing {
        g.fill_rrect(Rect::new(cx - 2.6, cy + 6.6, 5.2, if reacting { 2.4 } else { 1.2 }), 1.0, if reacting { pearl.fade(0.8) } else { blue });
        if reacting || playful {
            for side in [-1.0f32, 1.0] {
                g.fill_circle(cx + side * 10.0, cy + 3.8, 2.0, blue.fade(ca * 0.35));
            }
        }
    } else if a.timer_running() {
        g.fill_rrect(Rect::new(cx - 2.6, cy + 5.7, 5.2, 1.0), 0.5, pearl.fade(0.55));
    }
    // Small contextual details give the companion a role without adding UI
    // chrome. These travel with the pill and stay subtle while idle.
    if playful && !reacting {
        // Three tiny moods alternate on tap: a sparkle, a heart and a wink.
        let pop = 0.65 + 0.35 * (t * 5.0).sin().abs();
        if a.mascot_reaction_kind == 1 {
            g.fill_circle(cx + 13.0, cy - 9.0, 2.0, Color(1.0, 0.52, 0.66, ca * pop));
            g.fill_circle(cx + 16.0, cy - 11.0, 1.1, pearl.fade(pop));
        } else if a.mascot_reaction_kind == 2 {
            g.fill_circle(cx - 13.0, cy - 9.0, 2.0, blue.fade(pop));
            g.fill_circle(cx + 13.0, cy - 9.0, 2.0, blue.fade(pop));
        } else {
            g.fill_rrect(Rect::new(cx + 11.0, cy - 8.0, 5.0, 1.4), 0.7, pearl.fade(pop));
        }
    } else if playing && !reacting {
        // Two tiny floating notes, drawn as geometry so the mascot never
        // depends on an emoji font. They drift at different speeds and fade
        // naturally, keeping the resting island understated.
        for (i, side) in [(0.0f32, 1.0f32), (1.0, -1.0)] {
            let phase = (t * (0.65 + i * 0.15) + i * 1.7).rem_euclid(3.3) / 3.3;
            let x = cx + side * (14.0 + 2.0 * (phase * std::f32::consts::PI).sin());
            let y = cy - 7.0 - phase * 10.0;
            let opacity = (phase * 5.0).min(1.0) * ((1.0 - phase) * 2.2).min(1.0);
            let ink = blue.fade(ca * opacity * 0.65);
            g.fill_circle(x, y + 3.5, 1.7, ink);
            g.line((x + 1.4, y + 3.2), (x + 1.4, y - 3.7), 1.1, ink);
            g.line((x + 1.4, y - 3.7), (x + 4.1, y - 2.8), 1.1, ink);
        }
    } else if a.timer_running() && !reacting {
        let pulse = 0.35 + 0.25 * (t * 1.4).sin().abs();
        g.fill_circle(cx + 12.0, cy - 8.0, 1.4, blue.fade(pulse));
    }
}

/// The state glyph at the left of the idle and timer notch. Shape differs by state, not just colour.
fn draw_state_dot(a: &App, ch: f32, ca: f32) {
    let g = &a.gfx;
    let (cx, cy) = (13.0, ch / 2.0);
    let blue = Color(DEFAULT_ACCENT.0, DEFAULT_ACCENT.1, DEFAULT_ACCENT.2, ca);
    match a.dot_state() {
        DotState::Idle => g.fill_circle(cx, cy, 3.0, blue),
        DotState::Mic | DotState::MicMuted => {
            let c = if a.dot_state() == DotState::MicMuted { Color(1.0, 0.3, 0.3, ca) } else { blue };
            g.fill_circle(cx, cy, 4.2, c);
            g.fill_circle(cx, cy, 2.6, BLACK);
        }
        DotState::Battery => {
            let amber = Color(1.0, 0.72, 0.2, ca);
            g.fill_rrect(Rect::new(cx - 4.0, cy - 4.0, 8.0, 8.0), 1.8, amber);
            g.fill_rrect(Rect::new(cx - 0.55, cy - 2.6, 1.1, 3.2), 0.5, BLACK);
            g.fill_circle(cx, cy + 2.0, 0.6, BLACK);
        }
        DotState::Focus => {
            let done = focus_fraction(a.timer_total_s, a.timer_remaining_ms());
            let col = if a.break_mode { Color(0.12, 0.72, 0.62, ca) } else { blue };
            let steps = 28;
            let pt = |i: usize| {
                let ang = -std::f32::consts::FRAC_PI_2 + i as f32 / steps as f32 * std::f32::consts::TAU;
                (cx + 4.4 * ang.cos(), cy + 4.4 * ang.sin())
            };
            for i in 0..steps {
                g.line(pt(i), pt(i + 1), 1.6, Color(0.2, 0.2, 0.22, ca));
            }
            for i in 0..((done * steps as f32).round() as usize).min(steps) {
                g.line(pt(i), pt(i + 1), 1.8, col);
            }
        }
    }
}

/// How much of a timer session has passed, 0 to 1.
pub fn focus_fraction(total_s: u32, remaining_ms: i64) -> f32 {
    let total = (total_s as f32 * 1000.0).max(1.0);
    (1.0 - remaining_ms as f32 / total).clamp(0.0, 1.0)
}

/// A 2 px line along the inside bottom edge of the pill: how much of the focus session has passed.
fn draw_focus_line(a: &App, w: f32, ch: f32, ca: f32) {
    let g = &a.gfx;
    let done = focus_fraction(a.timer_total_s, a.timer_remaining_ms());
    let (x, len, y) = (14.0, (w - 28.0).max(1.0), ch - 5.0);
    g.fill_rrect(Rect::new(x, y, len, 2.0), 1.0, Color(0.15, 0.15, 0.165, ca));
    if done > 0.0 {
        // the existing blue; breaks use the teal of the break nudge
        let c = if a.break_mode { Color(0.12, 0.72, 0.62, ca) } else { Color(DEFAULT_ACCENT.0, DEFAULT_ACCENT.1, DEFAULT_ACCENT.2, ca) };
        g.fill_rrect(Rect::new(x, y, (len * done).max(2.0), 2.0), 1.0, c);
    }
}

fn draw_bars(a: &App, x: f32, cy: f32, ca: f32) {
    let g = &a.gfx;
    let playing = a.mstate.playing;
    let col = a.accent.fade(ca * if playing { 1.0 } else { 0.45 });
    for i in 0..4 {
        let hgt = if playing && a.cfg.eq_animation {
            4.0 + 10.0 * (0.5 + 0.5 * (a.eq_phase + i as f32 * 1.7).sin())
        } else if playing {
            [9.0, 13.0, 8.0, 11.0][i]
        } else {
            [5.0, 8.0, 6.0, 4.0][i]
        };
        let bx = x + i as f32 * 5.5;
        g.fill_rrect(Rect::new(bx, cy - hgt * 0.5, 3.0, hgt), 1.5, col);
    }
}

// -------------------------------------------------------------- expanded ----

fn hot(a: &App, act: Act) -> bool {
    a.hot == Some(act)
}

/// How hovered a control is, 0..1 (eases in and out instead of snapping).
fn hot_k(a: &App, act: Act) -> f32 {
    a.hover.iter().find(|(x, _)| *x == act).map_or(0.0, |(_, v)| *v)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t)
}

/// The hover squash of the collapsed pill, same keyframes as the website's notch:
/// wide and short, then narrow and tall, then back. `jiggle` runs 1 -> 0.
pub fn squash(jiggle: f32) -> (f32, f32) {
    let p = 1.0 - jiggle.clamp(0.0, 1.0);
    let (a, b, t) = if p < 0.3 {
        ((1.0, 1.0), (1.05, 0.92), p / 0.3)
    } else if p < 0.6 {
        ((1.05, 0.92), (0.98, 1.04), (p - 0.3) / 0.3)
    } else {
        ((0.98, 1.04), (1.0, 1.0), (p - 0.6) / 0.4)
    };
    let k = t * t * (3.0 - 2.0 * t);
    (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k)
}

const TAB_LABELS: [&str; 10] = ["Home", "Media", "Activity", "Agenda", "Shelf", "Clips", "Apps", "Tools", "Stats", "Power"];
const TAB_ICONS: [char; 10] = [
    '\u{E80F}', // home
    '\u{E8D6}', // music
    '\u{E7F4}', // notification activity
    '\u{E787}', // calendar
    '\u{E8B7}', // files
    '\u{E8C8}', // clipboard
    '\u{E71D}', // apps
    '\u{E713}', // tools
    '\u{E9D9}', // status
    '\u{E7E8}', // power
];

/// Two levels: five primary destinations, then five compact utilities.
pub fn tab_rects() -> Vec<Rect> {
    (0..10).map(|i| if i < 5 {
        Rect::new(48.0 + i as f32 * 101.0, 7.0, 96.0, 30.0)
    } else {
        Rect::new(73.0 + (i - 5) as f32 * 91.0, 41.0, 86.0, 23.0)
    }).collect()
}

fn draw_expanded(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;

    // Drill-down panels don't add tiny tabs to the already crowded tab row.
    if matches!(a.page, Page::Processes | Page::Calculator | Page::Typing | Page::WindowManager | Page::Mirror | Page::ShelfPreview | Page::Weather | Page::Tasks | Page::Assistant) {
        let back = Rect::new(18.0, 8.0, 80.0, 28.0);
        hits.push(Hit { r: back, act: Act::SubpageBack });
        g.fill_rrect(back, 12.0, HOVER.fade(ca));
        g.text("‹ Back", back, &f.small, T1.fade(ca), Align::Center);
        let title = match a.page { Page::Assistant => "Quick Notion task", Page::Processes => "Task Manager", Page::WindowManager => "Window Layouts", Page::Mirror => "Mirror", Page::ShelfPreview => "Quick Look", Page::Activity => "Activity Center", Page::Weather => "Weather", Page::Tasks => "Focus & Tasks", Page::Typing => "Typing Sprint", _ => "Calculator" };
        g.text(title, Rect::new(115.0, 8.0, EXP_W - 230.0, 28.0), &f.title, T1.fade(ca), Align::Center);
        let start = hits.len();
        g.shifted(0.0, CONTENT_Y, || match a.page {
            Page::Processes => page_processes(a, ca, hits),
            Page::WindowManager => page_window_manager(a, ca, hits),
            Page::Mirror => page_mirror(a, ca),
            Page::ShelfPreview => page_shelf_preview(a, ca, hits),
            Page::Activity => page_activity(a, ca, hits),
            Page::Weather => page_weather(a, ca),
            Page::Tasks => page_tasks(a, ca, hits),
            Page::Typing => page_typing(a, ca, hits),
            Page::Assistant => page_assistant(a, ca, hits),
            _ => page_calculator(a, ca, hits),
        });
        for h in &mut hits[start..] { h.r.y += CONTENT_Y; }
        return;
    }

    // top row: pin, labelled tabs, collapse. The pages sit below it.
    let pin = Rect::new(12.0, 8.0, 28.0, 28.0);
    hits.push(Hit { r: pin, act: Act::Pin });
    let pc = if a.pinned { a.accent } else if hot(a, Act::Pin) { T1 } else { T3 };
    g.icon('\u{E718}', pin, &f.icon14, pc.fade(ca));

    let chev = Rect::new(EXP_W - 40.0, 8.0, 28.0, 28.0);
    hits.push(Hit { r: chev, act: Act::Collapse });
    g.icon('\u{E70E}', chev, &f.icon14, (if hot(a, Act::Collapse) { T1 } else { T3 }).fade(ca));

    // Main journeys are prominent; utility pages stay one click away below.
    let rects = tab_rects();
    // Five individual destinations above a quieter utility strip. Their
    // hit boxes remain stable while the visual language differs from pills.
    g.fill_rrect(Rect::new(67.0, 40.0, EXP_W - 134.0, 24.0), 8.0, Color::white(0.035 * ca));
    let active_index = PAGES.iter().position(|p| *p == a.page).unwrap_or(0);
    let pill = Rect::new(a.tab_x, if active_index < 5 { 7.0 } else { 41.0 }, a.tab_w, if active_index < 5 { 30.0 } else { 23.0 });
    if active_index >= 5 {
        g.fill_rrect(pill, 7.0, Color::white(0.10 * ca));
    }
    for (i, p) in PAGES.iter().enumerate() {
        let r = rects[i];
        let act = Act::Tab(*p);
        hits.push(Hit { r, act });
        let k = hot_k(a, act);
        if i < 5 {
            glass_card(a, r, 10.0, ca * if a.page == *p { 0.8 } else { 0.42 + 0.3 * k }, a.page == *p || k > 0.5);
            if a.page == *p {
                g.fill_rrect(Rect::new(r.x + 12.0, r.y + r.h - 3.0, r.w - 24.0, 2.0), 1.0, glass_accent(a).fade(ca));
            }
        } else if a.page != *p {
            g.fill_rrect(r, 7.0, Color::white(0.06 * k * ca));
        }
        let selected = a.page == *p;
        let col = if selected { T1 } else { mix(Color::white(if i < 5 { 0.67 } else { 0.53 }), T1, k) };
        if i < 5 {
            g.icon(TAB_ICONS[i], Rect::new(r.x + 11.0, r.y + 6.0, 21.0, 18.0), &f.icon14, col.fade(ca));
            g.text(TAB_LABELS[i], Rect::new(r.x + 34.0, r.y + 5.0, r.w - 39.0, 20.0), &f.small, col.fade(ca), Align::Left);
        } else {
            g.icon(TAB_ICONS[i], Rect::new(r.x + 5.0, r.y + 2.0, 19.0, r.h - 4.0), &f.icon14, col.fade(ca));
            g.text(TAB_LABELS[i], Rect::new(r.x + 26.0, r.y, r.w - 28.0, r.h), &f.tiny, col.fade(ca), Align::Left);
        }
    }

    // the page itself, moved below the tab row; its hit boxes move with it
    let start = hits.len();
    let draw_page = |p: Page, ca: f32, hits: &mut Vec<Hit>| match p {
        Page::Media => page_media(a, ca, hits),
        Page::Agenda => page_agenda(a, ca),
        Page::Controls => page_controls(a, ca, hits),
        Page::Clipboard => page_clipboard(a, ca, hits),
        Page::Apps => page_apps(a, ca, hits),
        Page::Shelf => page_shelf(a, ca, hits),
        Page::Tasks => page_tasks(a, ca, hits),
        Page::Stats => page_stats(a, ca, hits),
        Page::Power => page_power(a, ca, hits),
        Page::Calculator => page_calculator(a, ca, hits),
        Page::Typing => page_typing(a, ca, hits),
        Page::WindowManager => page_window_manager(a, ca, hits),
        Page::Mirror => page_mirror(a, ca),
        Page::ShelfPreview => page_shelf_preview(a, ca, hits),
        Page::Activity => page_activity(a, ca, hits),
        Page::Weather => page_weather(a, ca),
        Page::Processes => page_processes(a, ca, hits),
        Page::Home => page_home(a, ca, hits),
        Page::Assistant => page_assistant(a, ca, hits),
        Page::Game => {
            let (ox, oy) = ((EXP_W - crate::game::W) / 2.0, 14.0);
            if a.hub.is_playing() {
                hits.push(Hit { r: Rect::new(ox, oy, crate::game::W, crate::game::H), act: Act::GameClick });
                a.hub.draw(g, ox, oy, ca);
                let r = Rect::new(ox + crate::game::W / 2.0 - 36.0, oy + 3.0, 72.0, 18.0);
                hits.push(Hit { r, act: Act::GameBack });
                g.fill_rrect(r, 9.0, (if hot(a, Act::GameBack) { HOVER } else { Color(0.0, 0.0, 0.0, 0.35) }).fade(ca));
                g.text("\u{2039} Games", r, &f.tiny, T1.fade(ca), Align::Center);
            } else {
                let hot_tile = (0..4).find(|i| hot(a, Act::GamePick(*i)));
                a.hub.draw_hub(g, ox, oy, ca, hot_tile);
                for (i, r) in crate::games::Hub::tiles(ox, oy).iter().enumerate() {
                    hits.push(Hit { r: *r, act: Act::GamePick(i) });
                }
            }
        }
    };
    if a.page_t < 1.0 {
        // cross-fade: the old page drifts up and fades out (not clickable), the new one rises in
        // Smoothstep has zero velocity at both ends, preventing a visible
        // snap when a tab transition begins or completes.
        let k = a.page_t * a.page_t * (3.0 - 2.0 * a.page_t);
        let mut gone = Vec::new();
        g.shifted(0.0, CONTENT_Y - 6.0 * k, || draw_page(a.prev_page, ca * (1.0 - k), &mut gone));
        g.shifted(0.0, CONTENT_Y + 8.0 * (1.0 - k), || draw_page(a.page, ca * k, hits));
    } else {
        g.shifted(0.0, CONTENT_Y, || draw_page(a.page, ca, hits));
    }
    for h in &mut hits[start..] {
        h.r.y += CONTENT_Y;
    }
}

// ----------------------------------------------------------------- media ----

/// At-a-glance landing page. Music gets the hero card while the right rail
/// presents two independent live widgets. All actions remain one click away.
fn page_home(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let (year, month, day, _, hh, mm, _, _) = sys::local_time();
    let music = Rect::new(18.0, 6.0, 350.0, 136.0);
    let weather_card = Rect::new(380.0, 6.0, 202.0, 63.0);
    let overview = Rect::new(380.0, 77.0, 202.0, 65.0);
    // The entire Now Playing card opens the full media player. Individual
    // transport and volume hits are added later and take precedence.
    hits.push(Hit { r: music, act: Act::Tab(Page::Media) });
    hits.push(Hit { r: weather_card, act: Act::WeatherOpen });
    hits.push(Hit { r: overview, act: Act::Tab(Page::Agenda) });
    // One continuous composition instead of three competing dark-blue cards.
    // Hover is a material shift, not a thick outline.
    let hero_hover = hot(a, Act::Tab(Page::Media));
    glass_card(a, music, 22.0, ca, hero_hover);
    glass_card(a, Rect::new(380.0, 6.0, 202.0, 136.0), 22.0, ca, hot(a, Act::WeatherOpen) || hot(a, Act::Tab(Page::Agenda)));
    g.fill_rrect(Rect::new(392.0, 73.0, 178.0, 1.0), 0.5, Color::white(0.12 * ca));
    if hot(a, Act::WeatherOpen) { g.fill_rrect(weather_card, 20.0, Color::white(0.055 * ca)); }
    if hot(a, Act::Tab(Page::Agenda)) { g.fill_rrect(overview, 20.0, Color::white(0.055 * ca)); }
    g.fill_rrect(Rect::new(35.0, 7.0, 306.0, 1.0), 0.5, if a.art.is_some() { a.accent.fade(0.38 * ca) } else { Color::white(0.12 * ca) });
    if let Some(art) = &a.art {
        g.draw_bitmap(art, Rect::new(28.0, 18.0, 90.0, 90.0), 16.0, ca);
    } else if a.mascot_visible {
        draw_miso_at(a, ca, 73.0, 62.0, true);
    } else {
        g.icon('\u{E8D6}', Rect::new(32.0, 28.0, 74.0, 66.0), &f.icon22, a.accent.fade(ca));
    }
    g.text("NOW PLAYING", Rect::new(128.0, 15.0, 212.0, 18.0), &f.tiny, T3.fade(ca), Align::Left);
    let song = if a.mstate.has_track() { a.mstate.title.as_str() } else { "Nothing playing" };
    g.text(song, Rect::new(128.0, 35.0, 208.0, 25.0), &f.body, T1.fade(ca), Align::Left);
    g.text(&a.mstate.artist, Rect::new(128.0, 60.0, 208.0, 17.0), &f.tiny, T2.fade(ca), Align::Left);
    for (r, act, icon) in [
        (Rect::new(154.0, 82.0, 27.0, 27.0), Act::Prev, '\u{E892}'),
        (Rect::new(201.0, 80.0, 31.0, 31.0), Act::PlayPause, if a.mstate.playing { '\u{E769}' } else { '\u{E768}' }),
        (Rect::new(252.0, 82.0, 27.0, 27.0), Act::Next, '\u{E893}'),
    ] {
        hits.push(Hit { r, act });
        if hot(a, act) { g.fill_rrect(r, 11.0, HOVER.fade(ca)); }
        if act == Act::PlayPause { g.fill_circle(r.cx(), r.cy(), 16.0, Color::white(0.14 * ca)); }
        g.icon(icon, r, &f.icon14, T1.fade(ca));
    }
    if let Some(v) = a.spotify_vol {
        let mute = Rect::new(129.0, 113.0, 21.0, 21.0);
        hits.push(Hit { r: mute, act: Act::SpotifyMute });
        g.icon(if v < 0.001 { '\u{E74F}' } else { '\u{E767}' }, mute, &f.icon14, T2.fade(ca));
        slider(a, Rect::new(157.0, 117.0, 144.0, 14.0), v, Act::SpotifyVolume, a.accent, ca, hits, true);
        g.text(&format!("{}%", (v * 100.0).round() as u32), Rect::new(307.0, 115.0, 37.0, 18.0), &f.tiny, T2.fade(ca), Align::Right);
    }
    let weather = crate::weather::snapshot();
    g.text("RIGHT NOW", Rect::new(394.0, 11.0, 115.0, 17.0), &f.tiny, a.accent.fade(ca), Align::Left);
    g.text(&format!("{hh:02}:{mm:02}"), Rect::new(394.0, 28.0, 101.0, 32.0), &f.title, T1.fade(ca), Align::Left);
    g.text(&format!("{day:02}/{month:02}/{:02}", year.rem_euclid(100)), Rect::new(394.0, 56.0, 91.0, 13.0), &f.tiny, T3.fade(ca), Align::Left);
    let wx_label = weather.temperature.map_or_else(|| "Weather +".to_string(), |t| format!("{}°", t.round() as i32));
    g.text(&wx_label, Rect::new(486.0, 24.0, 82.0, 33.0), &f.title, T1.fade(ca), Align::Right);
    g.text(if weather.city.is_empty() { "Set a city" } else { &weather.city }, Rect::new(484.0, 51.0, 84.0, 14.0), &f.tiny, T3.fade(ca), Align::Right);
    g.text("UP NEXT", Rect::new(394.0, 82.0, 173.0, 18.0), &f.tiny, a.accent.fade(ca), Align::Left);
    let (now, _) = crate::cal::now();
    let next = crate::cal::LABELED_EVENTS.lock().ok().and_then(|events| {
        events.iter().find(|e| e.event.start >= if e.event.all_day { now.div_euclid(86400) * 86400 } else { now - 60 }).cloned()
    });
    if let Some(entry) = next {
        const CAL_COLORS: [Color; 6] = [
            Color(0.52, 0.74, 1.0, 1.0), Color(0.95, 0.61, 0.45, 1.0),
            Color(0.61, 0.85, 0.53, 1.0), Color(0.78, 0.63, 0.98, 1.0),
            Color(0.99, 0.78, 0.39, 1.0), Color(0.45, 0.83, 0.80, 1.0),
        ];
        let event_h = entry.event.start.rem_euclid(86400) / 3600;
        let event_m = entry.event.start.rem_euclid(3600) / 60;
        let days_away = (entry.event.start.div_euclid(86400) - now.div_euclid(86400)).max(0);
        let when = match days_away {
            0 => "Today".to_string(),
            1 => "Tomorrow".to_string(),
            n => format!("In {n} days"),
        };
        let col = CAL_COLORS[entry.color_index % CAL_COLORS.len()];
        g.fill_circle(397.0, 111.0, 3.5, col.fade(ca));
        g.text(&entry.calendar, Rect::new(408.0, 101.0, 158.0, 20.0), &f.tiny, col.fade(ca), Align::Left);
        let time_label = if entry.event.all_day { "All day".to_string() } else { format!("{event_h:02}:{event_m:02}") };
        g.text(&format!("{when} {time_label} · {}", entry.event.title), Rect::new(394.0, 119.0, 176.0, 18.0), &f.tiny, T2.fade(ca), Align::Left);
    } else {
        g.text(&format!("{day:02}/{month:02} · Nothing upcoming"), Rect::new(394.0, 107.0, 171.0, 25.0), &f.tiny, T2.fade(ca), Align::Left);
    }

    let shortcuts = [
        ("Clipboard", "Recent copied items".into(), Page::Clipboard, '\u{E8C8}'),
        ("Shelf", "Drop and retrieve files".into(), Page::Shelf, '\u{E8B7}'),
        ("Activity", format!("{} notifications", a.system_notices.len()), Page::Activity, '\u{E7F4}'),
        ("Focus", if a.timer_running() { "Timer running".into() } else { "Timer & to-do list".into() }, Page::Tasks, '\u{E823}'),
    ];
    let dock = Rect::new(18.0, 153.0, 564.0, 48.0);
    glass_card(a, dock, 18.0, ca, false);
    for (i, (title, _sub, page, glyph)) in shortcuts.iter().enumerate() {
        let r = Rect::new(18.0 + i as f32 * 141.0, 153.0, 141.0, 48.0);
        let act = Act::Tab(*page);
        hits.push(Hit { r, act });
        let hovered = hot(a, act);
        if hovered { g.fill_rrect(Rect::new(r.x + 3.0, r.y + 4.0, r.w - 6.0, 40.0), 15.0, HOVER.fade(ca)); }
        if i > 0 { g.fill_rrect(Rect::new(r.x, r.y + 12.0, 1.0, 23.0), 0.5, Color::white(0.10 * ca)); }
        g.icon(*glyph, Rect::new(r.x + 17.0, r.y + 13.0, 22.0, 21.0), &f.icon14, a.accent.fade(ca));
        g.text(title, Rect::new(r.x + 45.0, r.y + 11.0, 87.0, 25.0), &f.small, T1.fade(ca), Align::Left);
    }
    let ask = Rect::new(18.0, 211.0, 564.0, 32.0);
    hits.push(Hit { r: ask, act: Act::AssistantOpen });
    glass_card(a, ask, 12.0, ca * 0.7, hot(a, Act::AssistantOpen));
    g.icon('\u{E8F2}', Rect::new(32.0, 217.0, 24.0, 20.0), &f.icon14, glass_accent(a).fade(ca));
    g.text("Quick Notion task", Rect::new(65.0, 214.0, 300.0, 25.0), &f.small, T1.fade(ca), Align::Left);
    g.text("Capture an idea  ›", Rect::new(368.0, 214.0, 194.0, 25.0), &f.tiny, T3.fade(ca), Align::Right);
}

fn page_assistant(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let panel = Rect::new(20.0, 0.0, 560.0, 162.0);
    glass_card(a, panel, 18.0, ca, false);
    g.text("QUICK CAPTURE · NOTION", Rect::new(38.0, 14.0, 360.0, 18.0), &f.tiny, glass_accent(a).fade(ca), Align::Left);
    if !a.assistant_question.is_empty() {
        g.text(&a.assistant_question, Rect::new(38.0, 38.0, 520.0, 24.0), &f.small, T2.fade(ca), Align::Left);
    }
    let hebrew = a.assistant_answer.chars().any(|c| ('\u{0590}'..='\u{05ff}').contains(&c));
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in a.assistant_answer.split_whitespace() {
        if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > 42 {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() { line.push(' '); }
        line.push_str(word);
    }
    if !line.is_empty() { lines.push(line); }
    for (i, line) in lines.iter().skip(a.assistant_scroll).take(4).enumerate() {
        g.text(line, Rect::new(38.0, 68.0 + i as f32 * 21.0, 520.0, 21.0), &f.small, T1.fade(ca), if hebrew { Align::Right } else { Align::Left });
    }
    if lines.len() > 4 { g.text("SCROLL FOR MORE", Rect::new(390.0, 142.0, 164.0, 14.0), &f.tiny, T3.fade(ca), Align::Right); }
    let input = Rect::new(20.0, 176.0, 560.0, 48.0);
    hits.push(Hit { r: input, act: Act::AssistantAsk });
    glass_card(a, input, 16.0, ca, hot(a, Act::AssistantAsk));
    let prompt = if a.assistant_entry { a.entering.as_ref().map(|b| format!("{}│", String::from_utf16_lossy(b))).unwrap_or_default() }
        else if a.assistant_busy { "Saving to Notion...".into() }
        else { "Write a task in Hebrew or English...".into() };
    g.icon('\u{E8F2}', Rect::new(32.0, 188.0, 24.0, 24.0), &f.icon14, glass_accent(a).fade(ca));
    g.text(&prompt, Rect::new(65.0, 184.0, 500.0, 32.0), &f.small, T1.fade(ca), Align::Left);
    let notion = Rect::new(20.0, 231.0, 560.0, 25.0);
    if !a.assistant_question.is_empty() && !a.assistant_busy { hits.push(Hit { r: notion, act: Act::AssistantNotion }); }
    glass_card(a, notion, 9.0, ca * 0.7, hot(a, Act::AssistantNotion));
    g.text(if crate::notion::configured() { "Save task to Notion" } else { "Notion setup required · Run SETUP-NOTION.ps1" }, notion, &f.tiny, T2.fade(ca), Align::Center);
}

fn page_agenda(a: &App, ca: f32) {
    let g = &a.gfx;
    let f = &g.fonts;
    let (now, _) = crate::cal::now();
    // Calendar feeds are reloaded on restart. Never read the file every
    // animation frame just to decide whether the onboarding preview is shown.
    static HAS_EXTRA_FEEDS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let connected = !a.cfg.calendar_url.trim().is_empty()
        || *HAS_EXTRA_FEEDS.get_or_init(|| !crate::store::load_calendar_feeds().is_empty());
    g.fill_rrect(Rect::new(20.0, 2.0, 560.0, 42.0), 14.0, glass_accent(a).fade(0.12 * ca));
    g.fill_rrect(Rect::new(20.0, 2.0, 3.0, 42.0), 1.5, glass_accent(a).fade(ca));
    if !connected {
        g.text("YOUR DAY", Rect::new(35.0, 5.0, 145.0, 15.0), &f.tiny, glass_accent(a).fade(ca), Align::Left);
        g.text("Connect a calendar", Rect::new(35.0, 19.0, 300.0, 21.0), &f.body, T1.fade(ca), Align::Left);
        g.text("EXAMPLE EVENTS", Rect::new(424.0, 13.0, 140.0, 18.0), &f.tiny, T3.fade(ca), Align::Right);
        for (i, (when, event, tint)) in [
            ("Today  15:00", "Web development", Color(0.50, 0.75, 1.0, 1.0)),
            ("Tomorrow", "Gym session", Color(0.49, 0.85, 0.64, 1.0)),
        ].iter().enumerate() {
            let y = 52.0 + i as f32 * 36.0;
            g.fill_circle(34.0, y + 15.0, 4.0, tint.fade(ca));
            g.fill_rrect(Rect::new(47.0, y, 531.0, 31.0), 9.0, SURF.fade(ca));
            g.text(when, Rect::new(60.0, y + 4.0, 119.0, 23.0), &f.tiny, T2.fade(ca), Align::Left);
            g.text(event, Rect::new(185.0, y + 4.0, 364.0, 23.0), &f.small, T1.fade(ca), Align::Left);
        }
        g.text("These are examples. No calendar is connected.", Rect::new(48.0, 126.0, 530.0, 17.0), &f.tiny, T3.fade(ca), Align::Left);
        g.text("01  Copy your calendar's secret iCal URL", Rect::new(27.0, 151.0, 550.0, 18.0), &f.small, T2.fade(ca), Align::Left);
        g.text("02  Add Name|URL to %APPDATA%\\Nock\\calendars.txt", Rect::new(27.0, 172.0, 550.0, 18.0), &f.small, T2.fade(ca), Align::Left);
        g.text("03  Restart Notchix. Keep the URL private.", Rect::new(27.0, 193.0, 550.0, 18.0), &f.small, T2.fade(ca), Align::Left);
        return;
    }
    g.text("YOUR DAY", Rect::new(35.0, 5.0, 145.0, 15.0), &f.tiny, glass_accent(a).fade(ca), Align::Left);
    g.text("Up next", Rect::new(35.0, 19.0, 260.0, 21.0), &f.body, T1.fade(ca), Align::Left);
    g.text("READ ONLY · HOURLY SYNC", Rect::new(359.0, 15.0, 204.0, 17.0), &f.tiny, T3.fade(ca), Align::Right);
    let events = crate::cal::LABELED_EVENTS.lock().unwrap();
    let mut found = 0;
    for item in events.iter().filter(|entry| entry.event.start >= if entry.event.all_day { now.div_euclid(86400) * 86400 } else { now - 60 }).take(5) {
        let y = 49.0 + found as f32 * 31.0;
        found += 1;
        let r = Rect::new(48.0, y, EXP_W - 68.0, 28.0);
        g.fill_rrect(r, 8.0, (if found % 2 == 1 { SURF } else { Color::white(0.035) }).fade(ca));
        let color = match item.color_index % 6 {
            0 => Color(0.52, 0.74, 1.0, 1.0), 1 => Color(0.95, 0.61, 0.45, 1.0),
            2 => Color(0.61, 0.85, 0.53, 1.0), 3 => Color(0.78, 0.63, 0.98, 1.0),
            4 => Color(0.99, 0.78, 0.39, 1.0), _ => Color(0.45, 0.83, 0.80, 1.0),
        };
        g.fill_circle(33.0, y + 14.0, 4.0, color.fade(ca));
        let time = item.event.start.rem_euclid(86400);
        let days = (item.event.start.div_euclid(86400) - now.div_euclid(86400)).max(0);
        let day = match days { 0 => "Today".into(), 1 => "Tomorrow".into(), n => format!("In {n} days") };
        let time_label = if item.event.all_day { "All day".into() } else { format!("{:02}:{:02}", time / 3600, (time % 3600) / 60) };
        g.text(&format!("{} {}", day, time_label), Rect::new(60.0, y + 2.0, 116.0, 24.0), &f.tiny, T2.fade(ca), Align::Left);
        g.text(&item.event.title, Rect::new(180.0, y + 2.0, 247.0, 24.0), &f.small, T1.fade(ca), Align::Left);
        g.text(&item.calendar, Rect::new(435.0, y + 2.0, 130.0, 24.0), &f.tiny, color.fade(ca), Align::Right);
    }
    if found == 0 {
        g.text("Nothing scheduled in the next 14 days.", Rect::new(30.0, 76.0, EXP_W - 60.0, 33.0), &f.small, T2.fade(ca), Align::Center);
    }
    g.text("The calendar links remain on your PC. Events can't be edited here.", Rect::new(24.0, 209.0, EXP_W - 48.0, 16.0), &f.tiny, T3.fade(ca), Align::Center);
}

fn page_media(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    // a Previous / Next press that is still loading: keep showing the old track
    let loading = a.skip_loading();
    let m = loading.map_or(&a.mstate, |s| &s.snap);
    let art = loading.map_or(&a.art, |s| &s.art);

    if !m.has_track() {
        g.icon(
            '\u{E768}',
            Rect::new(0.0, 56.0, EXP_W, 40.0),
            &f.icon22,
            T3.fade(ca),
        );
        g.text(
            "Nothing playing",
            Rect::new(0.0, 98.0, EXP_W, 24.0),
            &f.body,
            T2.fade(ca),
            Align::Center,
        );
        g.text(
            "Start music or a video and it shows up here",
            Rect::new(0.0, 122.0, EXP_W, 20.0),
            &f.small,
            T3.fade(ca),
            Align::Center,
        );
        return;
    }

    if a.lyrics_view {
        page_media_lyrics(a, m, ca, hits);
        return;
    }

    let player_card = Rect::new(16.0, 8.0, EXP_W - 32.0, 196.0);
    glass_card(a, player_card, 24.0, ca, false);
    // A single quiet glass plane makes the artwork, track and controls read
    // as one player. Cover colour remains on text and the seek path only.
    g.fill_rrect(Rect::new(24.0, 16.0, EXP_W - 48.0, 180.0), 19.0, Color::white(0.025 * ca));

    // The full lyric view remains available; the current line is always
    // visible below the player when the source supplies lyrics.
    let lyrics_button = Rect::new(480.0, 163.0, 84.0, 25.0);
    hits.push(Hit { r: lyrics_button, act: Act::LyricsToggle });
    g.fill_rrect(lyrics_button, 12.0, (if hot(a, Act::LyricsToggle) { a.accent.fade(0.28) } else { Color::white(0.095) }).fade(ca));
    g.text("Lyrics ♪", lyrics_button, &f.small, a.accent.fade(ca), Align::Center);
    let lyric_strip = Rect::new(16.0, 209.0, EXP_W - 32.0, 33.0);
    glass_card(a, lyric_strip, 11.0, ca * 0.7, false);
    g.icon('\u{E8D6}', Rect::new(27.0, 214.0, 22.0, 23.0), &f.icon14, glass_accent(a).fade(ca));
    let lyric = crate::lyrics::snapshot()
        .filter(|s| s.key == crate::lyrics::key(&m.title, &m.artist))
        .map(|data| {
            if data.loading || data.lines.is_empty() { data.message }
            else {
                let (pos, _) = m.progress(sys::filetime_now());
                let index = if data.synced { data.lines.iter().rposition(|line| line.at.is_some_and(|at| at <= pos)).unwrap_or(0) } else { 0 };
                data.lines[index].text.clone()
            }
        }).unwrap_or_else(|| "Finding lyrics...".into());
    g.text(&lyric, Rect::new(56.0, 213.0, EXP_W - 83.0, 25.0), &f.small, T2.fade(ca), Align::Left);

    // artwork
    let ar = Rect::new(29.0, 23.0, 116.0, 116.0);
    match art {
        Some(bmp) => g.draw_bitmap(bmp, ar, 16.0, ca),
        None => {
            g.fill_rrect(ar, 16.0, Color::white(0.10 * ca));
            g.text("\u{266A}", ar, &f.big, T3.fade(ca), Align::Center);
        }
    }
    if loading.is_some() {
        g.fill_rrect(ar, 16.0, Color(0.0, 0.0, 0.0, 0.5 * ca));
        spinner(g, ar.cx(), ar.cy(), 18.0, a.spin_t, a.accent, ca);
    }
    g.stroke_rrect(ar, 16.0, glass_accent(a).fade(0.34 * ca), 1.0);

    // Spotify-specific volume from Windows Volume Mixer. This is not Spotify's
    // internal playback-volume setting, and does not change system master volume.
    // Query only while the media panel is in use to keep idle work near zero.
    if let Some(v) = a.spotify_vol {
        g.text("SPOTIFY MIXER", Rect::new(30.0, 157.0, 132.0, 17.0), &f.tiny, T3.fade(ca), Align::Left);
        let mute = Rect::new(28.0, 175.0, 22.0, 22.0);
        hits.push(Hit { r: mute, act: Act::SpotifyMute });
        if hot(a, Act::SpotifyMute) {
            g.fill_rrect(mute, 7.0, Color::white(0.10 * ca));
        }
        g.icon(if v < 0.001 { '\u{E74F}' } else { '\u{E767}' }, mute, &f.icon14, T2.fade(ca));
        let vr = Rect::new(56.0, 180.0, 83.0, 12.0);
        slider(a, vr, v, Act::SpotifyVolume, a.accent, ca, hits, true);
        g.text(&format!("{}%", (v * 100.0).round() as u32), Rect::new(141.0, 177.0, 27.0, 17.0), &f.tiny, T2.fade(ca), Align::Right);
    }

    let tx = 164.0;
    let tw = EXP_W - tx - 52.0;
    g.text("NOW PLAYING", Rect::new(tx, 28.0, tw, 16.0), &f.tiny, a.accent.fade(ca), Align::Left);
    let title = if m.title.is_empty() {
        "Unknown title"
    } else {
        &m.title
    };
    g.text(
        title,
        Rect::new(tx, 48.0, tw, 28.0),
        &f.title,
        T1.fade(ca),
        Align::Left,
    );
    let sub = match loading {
        Some(s) if s.next => "Loading next track\u{2026}",
        Some(_) => "Loading previous track\u{2026}",
        None if m.artist.is_empty() => &m.album,
        None => &m.artist,
    };
    g.text(
        sub,
        Rect::new(tx, 77.0, tw, 21.0),
        &f.body,
        (if loading.is_some() { a.accent } else { T2 }).fade(ca),
        Align::Left,
    );
    // Windows media sessions sometimes expose a hexadecimal application ID.
    // That is not useful information to show in a polished player.
    let friendly_app = !m.app.is_empty()
        && !(m.app.len() >= 12 && m.app.chars().all(|c| c.is_ascii_hexdigit()));
    g.text(if friendly_app { &m.app } else { "MEDIA SESSION" }, Rect::new(tx, 100.0, 285.0, 16.0), &f.tiny, T3.fade(ca), Align::Left);

    // progress
    let (pos, dur) = if loading.is_some() { (0.0, 0.0) } else { m.progress(sys::filetime_now()) };
    let frac = match a.seek_frac {
        Some(fr) => fr,
        None if dur > 0.0 => (pos / dur) as f32,
        None => 0.0,
    };
    let shown = if a.seek_frac.is_some() {
        frac as f64 * dur
    } else {
        pos
    };
    let bar = Rect::new(tx, 122.0, EXP_W - tx - 39.0, 18.0);
    wave_seek(a, bar, frac, ca, hits, dur > 0.0 && m.can_seek);
    g.text(
        &fmt_time(shown),
        Rect::new(tx, 140.0, 70.0, 16.0),
        &f.tiny,
        T3.fade(ca),
        Align::Left,
    );
    if dur > 0.0 {
        g.text(
            &fmt_time(dur),
            Rect::new(bar.x + bar.w - 70.0, 140.0, 70.0, 16.0),
            &f.tiny,
            T3.fade(ca),
            Align::Right,
        );
    }

    // transport
    let cx = tx + (EXP_W - tx - 39.0) / 2.0;
    let cy = 175.0;
    let prev = Rect::new(cx - 92.0, cy - 20.0, 40.0, 40.0);
    let play = Rect::new(cx - 26.0, cy - 26.0, 52.0, 52.0);
    let next = Rect::new(cx + 52.0, cy - 20.0, 40.0, 40.0);
    hits.push(Hit {
        r: prev,
        act: Act::Prev,
    });
    hits.push(Hit {
        r: play,
        act: Act::PlayPause,
    });
    hits.push(Hit {
        r: next,
        act: Act::Next,
    });
    for (r, act, glyph, ok) in [(prev, Act::Prev, '\u{E892}', m.can_prev), (next, Act::Next, '\u{E893}', m.can_next)] {
        if hot(a, act) {
            g.fill_rrect(r, 20.0, Color::white(0.10 * ca));
        }
        g.icon(glyph, r, &f.icon18, (if ok { T1 } else { T3 }).fade(ca));
    }
    let big = if hot(a, Act::PlayPause) { 1.0 } else { 0.92 };
    g.fill_circle(play.cx(), play.cy(), 24.0, Color(0.93, 0.91, 0.99, big * ca));
    g.stroke_rrect(Rect::new(play.cx() - 24.0, play.cy() - 24.0, 48.0, 48.0), 24.0, a.accent.fade(0.17 * ca), 1.2);
    let glyph = if m.playing { '\u{E769}' } else { '\u{E768}' };
    g.icon(glyph, play, &f.icon22, Color(0.0, 0.0, 0.0, ca));
}

/// Readable lyric lines with a permanently visible seek bar and transport.
/// No replacement of Prev, Play/Pause, Next, or mixer volume controls.
fn page_media_lyrics(a: &App, m: &crate::media::MediaState, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let lyrics_card = Rect::new(16.0, 5.0, EXP_W - 32.0, 199.0);
    g.fill_rrect(lyrics_card, 20.0, SURF.fade(ca));
    g.stroke_rrect(lyrics_card, 20.0, Color::white(0.085 * ca), 0.8);
    g.text(&m.title, Rect::new(28.0, 0.0, 390.0, 22.0), &f.small, T1.fade(ca), Align::Left);
    g.text(&m.artist, Rect::new(28.0, 20.0, 380.0, 16.0), &f.tiny, T3.fade(ca), Align::Left);
    let back = Rect::new(484.0, 3.0, 88.0, 27.0);
    hits.push(Hit { r: back, act: Act::LyricsToggle });
    g.fill_rrect(back, 12.0, (if hot(a, Act::LyricsToggle) { HOVER } else { SURF }).fade(ca));
    g.text("‹ Player", back, &f.small, a.accent.fade(ca), Align::Center);

    let display = Rect::new(25.0, 40.0, EXP_W - 50.0, 91.0);
    g.fill_rrect(display, 15.0, Color(0.0, 0.0, 0.0, 0.19 * ca));
    let data = crate::lyrics::snapshot().filter(|s| s.key == crate::lyrics::key(&m.title, &m.artist));
    if let Some(data) = data {
        if data.loading || data.lines.is_empty() {
            g.text(&data.message, display, &f.body, T2.fade(ca), Align::Center);
        } else {
            let (pos, _) = m.progress(sys::filetime_now());
            let active = if data.synced {
                data.lines.iter().rposition(|line| line.at.is_some_and(|at| at <= pos)).unwrap_or(0)
            } else { 0 };
            let start = if data.synced { active.saturating_sub(1).saturating_add(a.lyrics_scroll).min(data.lines.len().saturating_sub(1)) }
                        else { a.lyrics_scroll.min(data.lines.len().saturating_sub(1)) };
            for (i, line) in data.lines.iter().skip(start).take(3).enumerate() {
                let r = Rect::new(38.0, 46.0 + i as f32 * 27.0, EXP_W - 76.0, 25.0);
                let current = data.synced && start + i == active;
                if current { g.fill_rrect(r, 9.0, a.accent.fade(0.15 * ca)); }
                g.text(&line.text, r, &f.small, (if current { T1 } else { T2 }).fade(ca), Align::Center);
            }
        }
    } else {
        g.text("Finding lyrics...", display, &f.body, T2.fade(ca), Align::Center);
    }

    let (pos, dur) = m.progress(sys::filetime_now());
    let frac = a.seek_frac.unwrap_or_else(|| if dur > 0.0 { (pos / dur) as f32 } else { 0.0 });
    slider(a, Rect::new(28.0, 139.0, EXP_W - 56.0, 12.0), frac, Act::Seek, a.accent, ca, hits, dur > 0.0 && m.can_seek);
    g.text(&fmt_time(pos), Rect::new(28.0, 151.0, 70.0, 15.0), &f.tiny, T3.fade(ca), Align::Left);
    g.text(&fmt_time(dur), Rect::new(EXP_W - 98.0, 151.0, 70.0, 15.0), &f.tiny, T3.fade(ca), Align::Right);

    if let Some(v) = a.spotify_vol {
        let mute = Rect::new(27.0, 178.0, 22.0, 22.0);
        hits.push(Hit { r: mute, act: Act::SpotifyMute });
        g.icon(if v < 0.001 { '\u{E74F}' } else { '\u{E767}' }, mute, &f.icon14, T2.fade(ca));
        slider(a, Rect::new(55.0, 182.0, 96.0, 12.0), v, Act::SpotifyVolume, a.accent, ca, hits, true);
    }
    let cy = 185.0;
    for (r, act, glyph, allowed) in [
        (Rect::new(328.0, cy - 20.0, 40.0, 40.0), Act::Prev, '\u{E892}', m.can_prev),
        (Rect::new(391.0, cy - 24.0, 48.0, 48.0), Act::PlayPause, if m.playing { '\u{E769}' } else { '\u{E768}' }, true),
        (Rect::new(462.0, cy - 20.0, 40.0, 40.0), Act::Next, '\u{E893}', m.can_next),
    ] {
        hits.push(Hit { r, act });
        if hot(a, act) { g.fill_rrect(r, 20.0, HOVER.fade(ca)); }
        if act == Act::PlayPause { g.fill_circle(r.cx(), r.cy(), 22.0, Color::white(ca)); }
        g.icon(glyph, r, &f.icon18, (if act == Act::PlayPause { BLACK } else if allowed { T1 } else { T3 }).fade(ca));
    }
}

// -------------------------------------------------------------- controls ----

/// One rounded tile: round icon, title, small state line. `on` fills it with the accent.
#[allow(clippy::too_many_arguments)]
fn tile(a: &App, ca: f32, hits: &mut Vec<Hit>, r: Rect, act: Act, glyph: char, title: &str, sub: &str, on: bool) {
    let g = &a.gfx;
    let f = &g.fonts;
    hits.push(Hit { r, act });
    let hovered = hot(a, act);
    if on { g.fill_rrect(r, 18.0, a.accent.fade(ca)); }
    else { glass_card(a, r, 18.0, ca, hovered); }
    if on && hovered {
        g.fill_rrect(r, 18.0, Color::white(0.10 * ca));
    }
    let ic = Rect::new(r.x + 12.0, r.y + (r.h - 34.0) / 2.0, 34.0, 34.0);
    let tint = match glyph { '\u{E722}' => Color(0.98, 0.45, 0.55, 1.0), '\u{E7F4}' => C_MEM, '\u{E767}' => C_UP, '\u{E774}' => C_CPU, '\u{E83F}' => C_BAT, _ => T1 };
    g.fill_circle(ic.cx(), ic.cy(), 17.0, (if on { Color(0.0, 0.0, 0.0, 0.14) } else { tint.fade(0.16) }).fade(ca));
    g.icon(glyph, ic, &f.icon14, (if on { BLACK } else { tint }).fade(ca));
    let (tc, sc) = if on { (BLACK, Color(0.0, 0.0, 0.0, 0.65)) } else { (T1, T3) };
    // title and state line stacked, centred beside the icon
    let ty = r.y + (r.h - 44.0) / 2.0;
    g.text(title, Rect::new(r.x + 56.0, ty, r.w - 64.0, 22.0), &f.body, tc.fade(ca), Align::Left);
    g.text(sub, Rect::new(r.x + 56.0, ty + 24.0, r.w - 64.0, 18.0), &f.tiny, sc.fade(ca), Align::Left);
}

fn page_controls(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    // Wi-Fi, Bluetooth, sound and brightness live in the top bar and Windows' own
    // quick settings, so this page is shortcuts to the screenshot tool and the
    // Settings pages people actually go looking for.
    let g = &a.gfx;
    g.text("COMMAND CENTER", Rect::new(22.0, 3.0, 235.0, 18.0), &g.fonts.tiny, glass_accent(a).fade(ca), Align::Left);
    g.text("Tools", Rect::new(22.0, 21.0, 205.0, 25.0), &g.fonts.title, T1.fade(ca), Align::Left);
    g.text("CAPTURE & AUDIO", Rect::new(22.0, 48.0, 205.0, 17.0), &g.fonts.tiny, T3.fade(ca), Align::Left);
    g.text("QUICK LAUNCH", Rect::new(222.0, 48.0, 205.0, 17.0), &g.fonts.tiny, T3.fade(ca), Align::Left);
    // The left column keeps the two most frequent actions large. The right
    // side is a compact command grid; all nine existing actions remain.
    tile(a, ca, hits, Rect::new(20.0, 68.0, 190.0, 73.0), Act::Quick(2), '\u{E722}', "Screenshot", "Capture region", false);
    tile(a, ca, hits, Rect::new(20.0, 149.0, 190.0, 73.0), Act::Quick(6), '\u{E767}', "Audio Switch", "Change output", false);
    let commands = [
        (Act::ProcessOpen, '\u{E9D9}', "Processes"),
        (Act::TypingOpen, '\u{E8D4}', "Typing Sprint"),
        (Act::OpenSettings(1), '\u{E767}', "Sound"),
        (Act::Quick(5), '\u{E720}', if a.mic_muted { "Mic: Muted" } else { "Microphone" }),
        (Act::CalculatorOpen, '\u{E8EF}', "Calculator"),
        (Act::WindowManagerOpen, '\u{E80F}', "Layouts"),
        (Act::MirrorOpen, '\u{E714}', "Mirror"),
        (Act::GlassToggle, '\u{E706}', if a.glass && a.live_glass.is_some() { "Live glass: On" } else if a.glass { "Glass: On" } else { "Glass: Off" }),
    ];
    for (i, (act, glyph, label)) in commands.iter().enumerate() {
        let col = (i % 2) as f32;
        let row = (i / 2) as f32;
        let r = Rect::new(222.0 + col * 181.0, 68.0 + row * 45.0, 173.0, 39.0);
        hits.push(Hit { r, act: *act });
        glass_card(a, r, 10.0, ca, hot(a, *act));
        g.fill_rrect(Rect::new(r.x, r.y + 6.0, 2.0, r.h - 12.0), 1.0, glass_accent(a).fade(0.55 * ca));
        g.icon(*glyph, Rect::new(r.x + 9.0, r.y + 7.0, 25.0, 25.0), &g.fonts.icon14, glass_accent(a).fade(ca));
        g.text(label, Rect::new(r.x + 38.0, r.y + 3.0, r.w - 43.0, 33.0), &g.fonts.small, T1.fade(ca), Align::Left);
    }
}

/// Nock-local notification history plus real live work already tracked by Nock.
/// Windows notifications from other apps are not silently intercepted.
fn page_activity(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let status = crate::shelf::archive_status();
    // A compact inbox header leaves enough room for three notices and a
    // two-column control deck with fully readable action labels.
    let permission = a.windows_listener.permission;
    let state = match permission {
        crate::win_notifications::Permission::Allowed => "Windows connected",
        crate::win_notifications::Permission::Pending => "Waiting for your permission...",
        crate::win_notifications::Permission::Denied => "Windows access denied",
        crate::win_notifications::Permission::Unspecified => "Permission required",
        crate::win_notifications::Permission::Unavailable => "Install Notchix to connect Windows alerts",
    };
    g.text("NOTIFICATIONS", Rect::new(24.0, 2.0, 230.0, 18.0), &f.tiny, glass_accent(a).fade(ca), Align::Left);
    g.fill_circle(29.0, 29.0, 3.0, (if permission == crate::win_notifications::Permission::Allowed { Color(0.30, 0.83, 0.60, 1.0) } else { T3 }).fade(ca));
    g.text(state, Rect::new(40.0, 20.0, 394.0, 18.0), &f.tiny, T3.fade(ca), Align::Left);
    let permission_button = Rect::new(447.0, 12.0, 122.0, 26.0);
    if permission != crate::win_notifications::Permission::Allowed && permission != crate::win_notifications::Permission::Pending {
        g.fill_rrect(permission_button, 13.5, a.accent.fade(0.42 * ca));
        g.text("Allow access", permission_button, &f.small, T1.fade(ca), Align::Center);
        hits.push(Hit { r: permission_button, act: Act::ActivityRequestAccess });
    }
    g.text("RECENT", Rect::new(25.0, 44.0, 140.0, 16.0), &f.tiny, glass_accent(a).fade(ca), Align::Left);
    let total = a.system_notices.len();
    let visible_from = if total == 0 { 0 } else { a.activity_scroll + 1 };
    let visible_to = (a.activity_scroll + 3).min(total);
    g.text(&format!("{visible_from}-{visible_to} of {total} · scroll"), Rect::new(400.0, 44.0, 173.0, 17.0), &f.tiny, T3.fade(ca), Align::Right);
    for (i, notice) in a.system_notices.iter().skip(a.activity_scroll).take(3).enumerate() {
        let y = 64.0 + i as f32 * 65.0;
        let r = Rect::new(20.0, y, 560.0, 58.0);
        let source = crate::win_notifications::display_source(&notice.app, &notice.title, &notice.body);
        let kind = crate::win_notifications::classify(&notice.app, &notice.title, &notice.body);
        let tint = notification_tint(source);
        // Layered graphite cards retain contrast without a bright, permanent
        // application-colored fill. The accent is limited to the icon.
        glass_card(a, r, 17.0, ca, false);
        if kind != crate::win_notifications::NoticeKind::Regular && !a.private_notifications {
            g.fill_rrect(Rect::new(23.0, y + 14.0, 2.0, 30.0), 1.0, tint.fade(0.8 * ca));
            let badge = Rect::new(386.0, y + 6.0, 98.0, 16.0);
            g.fill_rrect(badge, 8.0, tint.fade(0.13 * ca));
            g.text(kind.badge(), badge, &f.tiny, tint.fade(ca), Align::Center);
        }
        let icon = Rect::new(30.0, y + 13.0, 32.0, 32.0);
        draw_notice_icon(a, source, icon, ca);
        let age = notice.received.elapsed().as_secs();
        let time = if age < 60 { "now".to_string() } else if age < 3600 { format!("{}m", age / 60) } else { format!("{}h", age / 3600) };
        g.text(source, Rect::new(75.0, y + 7.0, 210.0, 14.0), &f.tiny, tint.fade(ca), Align::Left);
        g.text(&time, Rect::new(491.0, y + 8.0, 39.0, 14.0), &f.tiny, T3.fade(ca), Align::Right);
        // Dismiss only this Nock history row. Native Windows Action Center
        // notifications remain untouched by the local history controls.
        let dismiss = Rect::new(538.0, y + 17.0, 29.0, 24.0);
        let dismiss_act = Act::ActivityDismiss(a.activity_scroll + i);
        hits.push(Hit { r: dismiss, act: dismiss_act });
        if hot(a, dismiss_act) { g.fill_rrect(dismiss, 9.0, Color::white(0.14 * ca)); }
        g.icon('\u{E8BB}', dismiss, &f.icon14, T3.fade(ca));
        let title = if a.private_notifications { "Notification" } else { &notice.title };
        g.text(title, Rect::new(75.0, y + 23.0, 450.0, 17.0), &f.small, T1.fade(ca), Align::Left);
        if !notice.body.is_empty() {
            let body = if a.private_notifications { "Content hidden" } else { &notice.body };
            g.text(body, Rect::new(75.0, y + 43.0, 450.0, 14.0), &f.tiny, T2.fade(ca), Align::Left);
        }
    }
    if a.system_notices.is_empty() {
        if a.mascot_visible { draw_miso_at(a, ca, 300.0, 151.0, false); }
        g.text("Your notifications will appear here", Rect::new(20.0, 181.0, 560.0, 26.0), &f.small, T2.fade(ca), Align::Center);
    }

    let actions = [
        (Act::ActivityTestPreview, '\u{E7F4}', "Preview Island"),
        (Act::ActivityQuietToggle, '\u{E708}', if a.quiet_alerts { "Quiet alerts: ON" } else { "Quiet alerts: OFF" }),
        (Act::ActivityPrivacyToggle, '\u{E72E}', if a.private_notifications { "Private previews: ON" } else { "Private previews: OFF" }),
        (Act::ActivityClear, '\u{E74D}', "Clear all history"),
    ];
    for (i, (act, glyph, label)) in actions.iter().enumerate() {
        let r = Rect::new(20.0 + (i % 2) as f32 * 284.0, 256.0 + (i / 2) as f32 * 29.0, 276.0, 26.0);
        hits.push(Hit { r, act: *act });
        glass_card(a, r, 9.0, ca * 0.68, hot(a, *act));
        g.icon(*glyph, Rect::new(r.x + 9.0, r.y + 2.0, 22.0, 21.0), &f.icon14, glass_accent(a).fade(ca));
        g.text(label, Rect::new(r.x + 37.0, r.y, r.w - 48.0, r.h), &f.small, T1.fade(ca), Align::Left);
    }
    let progress = if status.busy && status.total_bytes > 0 { (status.done_bytes as f64 / status.total_bytes as f64).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let bar = Rect::new(20.0, 315.0, 560.0, 2.0);
    g.fill_rrect(bar, 1.5, HOVER.fade(ca));
    if progress > 0.0 { g.fill_rrect(Rect::new(bar.x, bar.y, bar.w * progress, bar.h), 1.5, a.accent.fade(ca)); }
    let _ = status;
}

/// Compact Pixel-inspired weather dashboard. Users choose their city in
/// %APPDATA%/Nock/weather.txt, no location access required.
fn page_weather(a: &App, ca: f32) {
    let g = &a.gfx;
    let f = &g.fonts;
    let w = crate::weather::snapshot();
    let hero = Rect::new(22.0, 6.0, 556.0, 147.0);
    g.fill_rrect(hero, 22.0, SURF.fade(ca));
    g.fill_rrect(Rect::new(22.0, 6.0, 556.0, 147.0), 22.0, a.accent.fade(0.08 * ca));
    g.stroke_rrect(hero, 22.0, a.accent.fade(0.21 * ca), 1.0);
    if let Some(temp) = w.temperature {
        let label = crate::weather::description(w.code);
        g.fill_circle(492.0, 62.0, 36.0, Color(1.0, 0.77, 0.37, 0.16 * ca));
        g.icon(if w.code >= 61 { '\u{E753}' } else if w.code == 0 { '\u{E706}' } else { '\u{E753}' }, Rect::new(453.0, 20.0, 78.0, 80.0), &f.icon22, a.accent.fade(ca));
        g.text(&w.city, Rect::new(45.0, 18.0, 290.0, 26.0), &f.body, T2.fade(ca), Align::Left);
        g.text(&format!("{}°", temp.round() as i32), Rect::new(42.0, 45.0, 230.0, 67.0), &f.big, T1.fade(ca), Align::Left);
        g.text(label, Rect::new(45.0, 111.0, 340.0, 25.0), &f.body, T1.fade(ca), Align::Left);
        if let Some(feels) = w.feels_like {
            g.text(&format!("Feels like {}°C", feels.round() as i32), Rect::new(35.0, 164.0, 246.0, 27.0), &f.small, T2.fade(ca), Align::Left);
        }
        if let Some(humidity) = w.humidity {
            g.text(&format!("Humidity {humidity}%"), Rect::new(315.0, 164.0, 242.0, 27.0), &f.small, T2.fade(ca), Align::Right);
        }
        g.text("Powered by Open-Meteo · updates about every 30 minutes", Rect::new(25.0, 198.0, 550.0, 17.0), &f.tiny, T3.fade(ca), Align::Center);
    } else {
        g.icon('\u{E753}', Rect::new(265.0, 20.0, 70.0, 65.0), &f.icon22, a.accent.fade(ca));
        g.text("Choose your weather location", Rect::new(48.0, 86.0, 504.0, 30.0), &f.body, T1.fade(ca), Align::Center);
        g.text("Edit %APPDATA%\\Nock\\weather.txt", Rect::new(42.0, 119.0, 516.0, 24.0), &f.small, T2.fade(ca), Align::Center);
        g.text("Add City|Latitude|Longitude, then reopen Weather", Rect::new(30.0, 165.0, 540.0, 28.0), &f.small, T2.fade(ca), Align::Center);
    }
}

fn page_mirror(a: &App, ca: f32) {
    let g = &a.gfx;
    let r = Rect::new(172.0, 5.0, 256.0, 174.0);
    g.fill_rrect(r, 18.0, SURF.fade(ca));
    if let Some(ref frame) = a.mirror_bitmap {
        // A real mirror, rather than a normal unflipped camera preview.
        g.scaled(r.cx(), r.cy(), -1.0, 1.0, || {
            g.draw_bitmap(frame, r, 16.0, ca);
        });
    } else {
        g.icon('\u{E714}', Rect::new(r.cx() - 25.0, r.cy() - 25.0, 50.0, 50.0), &g.fonts.icon22, a.accent.fade(ca));
    }
    let label = crate::mirror::status();
    g.text(&label, Rect::new(24.0, 183.0, EXP_W - 48.0, 20.0), &g.fonts.tiny, T2.fade(ca), Align::Center);
}

fn page_window_manager(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let names = ["Left", "Right", "Full", "Top left", "Top right", "Bottom left", "Bottom right", "Left third", "Middle third", "Right third"];
    for (i, title) in names.iter().enumerate() {
        let col = i % 5;
        let row = i / 5;
        let r = Rect::new(20.0 + col as f32 * 114.0, 18.0 + row as f32 * 82.0, 107.0, 72.0);
        let act = Act::SnapLayout(i as u8);
        hits.push(Hit { r, act });
        g.fill_rrect(r, 13.0, (if hot(a, act) { HOVER } else { SURF }).fade(ca));
        let icon = Rect::new(r.cx() - 29.0, r.y + 10.0, 58.0, 31.0);
        g.stroke_rrect(icon, 4.0, T3.fade(ca), 1.0);
        let fill = match i {
            0 => Rect::new(icon.x, icon.y, icon.w / 2.0, icon.h),
            1 => Rect::new(icon.cx(), icon.y, icon.w / 2.0, icon.h),
            2 => icon,
            3 => Rect::new(icon.x, icon.y, icon.w / 2.0, icon.h / 2.0),
            4 => Rect::new(icon.cx(), icon.y, icon.w / 2.0, icon.h / 2.0),
            5 => Rect::new(icon.x, icon.cy(), icon.w / 2.0, icon.h / 2.0),
            6 => Rect::new(icon.cx(), icon.cy(), icon.w / 2.0, icon.h / 2.0),
            7 => Rect::new(icon.x, icon.y, icon.w / 3.0, icon.h),
            8 => Rect::new(icon.x + icon.w / 3.0, icon.y, icon.w / 3.0, icon.h),
            _ => Rect::new(icon.x + icon.w * 2.0 / 3.0, icon.y, icon.w / 3.0, icon.h),
        };
        g.fill_rrect(fill, 3.0, a.accent.fade(0.60 * ca));
        g.text(title, Rect::new(r.x + 2.0, r.y + 48.0, r.w - 4.0, 17.0), &f.tiny, T1.fade(ca), Align::Center);
    }
    let overlay = Rect::new(130.0, 187.0, EXP_W - 260.0, 30.0);
    hits.push(Hit { r: overlay, act: Act::NativeSnapOverlay });
    g.fill_rrect(overlay, 10.0, (if hot(a, Act::NativeSnapOverlay) { a.accent.fade(0.34) } else { HOVER }).fade(ca));
    g.text("Open Windows 11 visual Snap Layouts (Win + Z)", overlay, &f.small, T1.fade(ca), Align::Center);
}

// ----------------------------------------------------------------- power ----

fn page_power(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let armed = |k: u8| matches!(a.power_confirm, Some((p, t)) if p == k && t.elapsed().as_secs_f32() < 3.5);
    let items: [(char, &str, Act, u8); 4] = [
        ('\u{E72E}', "Lock", Act::Quick(3), 0),
        ('\u{E708}', "Sleep", Act::Quick(4), 0),
        ('\u{E777}', "Restart", Act::PowerRestart, 1),
        ('\u{E7E8}', "Shut down", Act::PowerShutdown, 2),
    ];
    let (w, gap) = (129.0, 12.0);
    for (i, (glyph, label, act, k)) in items.iter().enumerate() {
        let r = Rect::new(24.0 + i as f32 * (w + gap), 28.0, w, 130.0);
        hits.push(Hit { r, act: *act });
        let danger = *k != 0 && armed(*k);
        // colour says how serious it is: calm blue and violet, amber for restart, red for shut down
        let tint = [C_CPU, C_UP, C_MEM, Color(1.0, 0.42, 0.45, 1.0)][i];
        let bg = if danger { Color(0.85, 0.25, 0.28, 1.0) } else if hot(a, *act) { HOVER } else { SURF };
        g.fill_rrect(r, 20.0, bg.fade(ca));
        if !danger {
            g.fill_rrect(r, 20.0, tint.fade((if hot(a, *act) { 0.14 } else { 0.07 }) * ca));
            g.fill_circle(r.cx(), r.y + 42.0, 24.0, tint.fade(0.16 * ca));
        }
        g.icon(*glyph, Rect::new(r.x, r.y + 22.0, r.w, 40.0), &f.icon22, (if danger { Color::white(1.0) } else { tint }).fade(ca));
        g.text(label, Rect::new(r.x, r.y + 70.0, r.w, 24.0), &f.body, (if danger { Color::white(1.0) } else { T1 }).fade(ca), Align::Center);
        if *k != 0 {
            g.text(
                if danger { "Click again to confirm" } else { "Asks to confirm" },
                Rect::new(r.x + 4.0, r.y + 96.0, r.w - 8.0, 20.0),
                &f.tiny,
                (if danger { Color::white(0.95) } else { T3 }).fade(ca),
                Align::Center,
            );
        }
    }
}

// ----------------------------------------------------------------- stats ----

/// Per-metric colours, so each stat reads at a glance.
const C_CPU: Color = Color(0.30, 0.78, 0.98, 1.0);
const C_MEM: Color = Color(0.98, 0.70, 0.20, 1.0);
const C_BAT: Color = Color(0.40, 0.85, 0.50, 1.0);
const C_UP: Color = Color(0.72, 0.58, 1.0, 1.0);
const C_FOC: Color = Color(1.0, 0.58, 0.42, 1.0);

/// One inset tile: coloured icon + dim label, bold value, thin bar underneath.
#[allow(clippy::too_many_arguments)]
fn stat_tile(g: &Gfx, r: Rect, glyph: char, label: &str, value: &str, note: &str, frac: Option<f32>, col: Color, ca: f32) {
    let f = &g.fonts;
    g.fill_rrect(r, 20.0, SURF.fade(ca));
    g.icon(glyph, Rect::new(r.x + 14.0, r.y + 12.0, 22.0, 22.0), &f.icon14, col.fade(ca));
    g.text(label, Rect::new(r.x + 42.0, r.y + 12.0, 120.0, 22.0), &f.small, T3.fade(ca), Align::Left);
    g.text(note, Rect::new(r.x + r.w - 150.0, r.y + 12.0, 136.0, 22.0), &f.tiny, T3.fade(ca), Align::Right);
    g.text(value, Rect::new(r.x + 16.0, r.y + 36.0, r.w - 32.0, 34.0), &f.big, T1.fade(ca), Align::Left);
    if let Some(fr) = frac {
        let bar = Rect::new(r.x + 16.0, r.y + r.h - 18.0, r.w - 32.0, 5.0);
        g.fill_rrect(bar, 2.5, Color::white(0.10 * ca));
        g.fill_rrect(Rect::new(bar.x, bar.y, (bar.w * fr.clamp(0.0, 1.0)).max(5.0), bar.h), 2.5, col.fade(ca));
    }
}

fn page_stats(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let (w, h, gap) = (280.0, 92.0, 12.0);
    let at = |c: f32, r: f32| Rect::new(14.0 + c * (w + gap), 6.0 + r * (h + gap), w, h);
    stat_tile(g, at(0.0, 0.0), '\u{E9D9}', "CPU", &format!("{}%", a.stat_cpu), "", Some(a.bar_cpu), C_CPU, ca);
    stat_tile(g, at(1.0, 0.0), '\u{E964}', "Memory", &format!("{}%", a.stat_mem), "", Some(a.bar_mem), C_MEM, ca);
    // CPU history beside the value
    let r = at(0.0, 0.0);
    let n = a.stat_hist.len();
    for (i, v) in a.stat_hist.iter().enumerate() {
        let bh = 2.0 + v.clamp(0.0, 1.0) * 22.0;
        g.fill_rrect(Rect::new(r.x + r.w - 16.0 - (n - i) as f32 * 4.6, r.y + 62.0 - bh, 3.0, bh), 1.5, C_CPU.fade(((i + 1) as f32 / n.max(1) as f32) * 0.7 * ca));
    }
    match sys::battery_info() {
        Some(b) => {
            let state = if b.charging { "Charging" } else if b.plugged { "Plugged in" } else { "On battery" };
            let col = if b.percent <= 20 && !b.charging { Color(1.0, 0.45, 0.4, 1.0) } else { C_BAT };
            stat_tile(g, at(0.0, 1.0), '\u{E83F}', "Battery", &format!("{}%", b.percent), state, Some(b.percent as f32 / 100.0), col, ca);
        }
        None => stat_tile(g, at(0.0, 1.0), '\u{E83F}', "Power", "Plugged in", "No battery", None, C_BAT, ca),
    }
    let up = sys::uptime_secs();
    let (d, hh, m) = (up / 86400, (up % 86400) / 3600, (up % 3600) / 60);
    let text = if d > 0 { format!("{d}d {hh}h {m}m") } else { format!("{hh}h {m}m") };
    // A visible monitor readout makes it possible to verify that Nock is
    // detecting actual I/O before assuming a particular app is supported.
    let transfer_note = if let Some(copy) = crate::explorer_copy::current() {
        let note = format!("Explorer {}%", (copy * 100.0).round() as u32);
        if let Some(q) = crate::qbit::current() { format!("{note} · qBit {}%", (q.percent * 100.0).round() as u32) } else { note }
    } else if let Some(q) = crate::qbit::current() {
        format!("qBit {}%", (q.percent * 100.0).round() as u32)
    } else if let Some(p) = crate::prism::current() {
        let percent = p.percent.map(|v| format!("{}%", (v * 100.0).round() as u32)).unwrap_or_else(|| "active".into());
        format!("Prism {percent}")
    } else if let Some(io) = crate::transfer::current() {
        format!("{} I/O", io.app)
    } else {
        "I/O idle".to_string()
    };
    stat_tile(g, at(1.0, 1.0), '\u{E916}', "Uptime", &text, &transfer_note, None, C_UP, ca);
    let r = Rect::new(448.0, 158.0, 122.0, 28.0);
    hits.push(Hit { r, act: Act::ProcessOpen });
    g.fill_rrect(r, 12.0, (if hot(a, Act::ProcessOpen) { HOVER } else { SURF }).fade(ca));
    g.text("Task Manager ›", r, &g.fonts.small, T1.fade(ca), Align::Center);
}

fn page_processes(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    g.text("Most demanding processes", Rect::new(22.0, 0.0, 265.0, 24.0), &f.small, T2.fade(ca), Align::Left);
    let toggle = Rect::new(323.0, 0.0, 145.0, 26.0);
    hits.push(Hit { r: toggle, act: Act::ProcessSort });
    g.fill_rrect(toggle, 12.0, SURF.fade(ca));
    g.text(if a.proc_sort_cpu { "Sort: CPU" } else { "Sort: RAM" }, toggle, &f.small, T1.fade(ca), Align::Center);
    let refresh = Rect::new(480.0, 0.0, 94.0, 26.0);
    hits.push(Hit { r: refresh, act: Act::ProcessRefresh });
    g.fill_rrect(refresh, 12.0, SURF.fade(ca));
    g.text("Refresh", refresh, &f.small, T1.fade(ca), Align::Center);
    for (i, p) in a.processes.iter().take(5).enumerate() {
        let y = 33.0 + i as f32 * 32.0;
        let r = Rect::new(18.0, y, EXP_W - 36.0, 30.0);
        g.fill_rrect(r, 9.0, (if i % 2 == 0 { SURF } else { Color::white(0.045) }).fade(ca));
        g.text(&p.name, Rect::new(28.0, y, 275.0, 30.0), &f.small, T1.fade(ca), Align::Left);
        g.text(&format!("{:.1}% CPU", p.cpu_pct), Rect::new(315.0, y, 94.0, 30.0), &f.tiny, C_CPU.fade(ca), Align::Right);
        g.text(&format!("{} MB", p.ram_mb), Rect::new(415.0, y, 70.0, 30.0), &f.tiny, C_MEM.fade(ca), Align::Right);
        let kill = Rect::new(495.0, y + 2.0, 80.0, 26.0);
        hits.push(Hit { r: kill, act: Act::ProcessEnd(p.pid) });
        let confirm = a.proc_confirm.is_some_and(|(pid, t)| pid == p.pid && t.elapsed().as_secs() < 4);
        g.fill_rrect(kill, 9.0, (if confirm { Color(0.75, 0.15, 0.2, 1.0) } else { HOVER }).fade(ca));
        g.text(if confirm { "Confirm" } else { "End task" }, kill, &f.tiny, T1.fade(ca), Align::Center);
    }
    g.text("Only active while this panel is open. End task can lose unsaved work.", Rect::new(20.0, 192.0, 555.0, 15.0), &f.tiny, T3.fade(ca), Align::Center);
}

fn page_calculator(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let display = Rect::new(133.0, 0.0, 334.0, 34.0);
    g.fill_rrect(display, 11.0, SURF.fade(ca));
    g.text(&a.calc_input, Rect::new(display.x + 10.0, 0.0, display.w - 20.0, 34.0), &f.title, T1.fade(ca), Align::Right);
    const KEYS: [&str; 4] = ["789/", "456*", "123-", "C0.="];
    for (row, keys) in KEYS.iter().enumerate() {
        for (col, key) in keys.chars().enumerate() {
            let r = Rect::new(133.0 + col as f32 * 85.0, 40.0 + row as f32 * 39.0, 78.0, 34.0);
            let act = Act::CalcKey(key);
            hits.push(Hit { r, act });
            let bg = if hot(a, act) { HOVER } else if "+-*/=".contains(key) { a.accent.fade(0.25) } else { SURF };
            g.fill_rrect(r, 10.0, bg.fade(ca));
            g.text(&key.to_string(), r, &f.body, T1.fade(ca), Align::Center);
        }
    }
    g.text("Use +, -, *, / and =. C clears the calculation.", Rect::new(110.0, 198.0, 390.0, 16.0), &f.tiny, T3.fade(ca), Align::Center);
}

// ------------------------------------------------------------- clipboard ----

fn page_typing(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let t = &a.typing;
    let remaining = (t.duration_s as f32 - t.elapsed()).ceil().max(0.0) as u32;
    let summary = Rect::new(18.0, 5.0, 564.0, 46.0);
    g.fill_rrect(summary, 15.0, SURF.fade(ca));
    g.text(&format!("{} WPM", t.wpm()), Rect::new(32.0, 10.0, 135.0, 32.0), &f.title, a.accent.fade(ca), Align::Left);
    g.text(&format!("{}% accuracy", t.accuracy()), Rect::new(189.0, 12.0, 162.0, 28.0), &f.small, T2.fade(ca), Align::Left);
    g.text(&format!("{}s left", remaining), Rect::new(419.0, 12.0, 143.0, 28.0), &f.small, T1.fade(ca), Align::Right);
    let progress = Rect::new(25.0, 59.0, 550.0, 3.0);
    g.fill_rrect(progress, 1.5, SURF.fade(ca));
    if t.started.is_some() {
        g.fill_rrect(Rect::new(progress.x, progress.y, progress.w * (t.elapsed() / t.duration_s as f32).clamp(0.0, 1.0), progress.h), 1.5, a.accent.fade(ca));
    }
    g.text("TYPE THIS", Rect::new(26.0, 72.0, 180.0, 18.0), &f.tiny, T3.fade(ca), Align::Left);
    g.fill_rrect(Rect::new(19.0, 93.0, 562.0, 87.0), 12.0, Color::white(0.08 * ca));
    // Render a rolling three-line window; the long text continues seamlessly.
    // Wrap on word boundaries so the practice paragraph stays readable.
    let mut lines: Vec<(usize, String)> = Vec::new();
    let mut line = String::new();
    let mut start = 0usize;
    for word in t.prompt().split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > 67 {
            lines.push((start, std::mem::take(&mut line)));
            start += lines.last().map_or(0, |(_, s)| s.len() + 1);
        }
        if !line.is_empty() { line.push(' '); }
        line.push_str(word);
    }
    if !line.is_empty() { lines.push((start, line)); }
    let typed_len = t.typed.len();
    let active = lines.iter().rposition(|(s, _)| *s <= typed_len).unwrap_or(0);
    let first = active.saturating_sub(1).min(lines.len().saturating_sub(3));
    for (i, (start, value)) in lines.iter().skip(first).take(3).enumerate() {
        let r = Rect::new(33.0, 100.0 + i as f32 * 25.0, 534.0, 24.0);
        g.text(value, r, &f.body, if *start <= typed_len { T1.fade(ca) } else { T3.fade(ca) }, Align::Left);
    }
    let input_color = if t.typed.chars().zip(t.prompt().chars()).all(|(c, expected)| c == expected) { Color(0.48, 0.89, 0.67, 1.0) } else { Color(0.97, 0.53, 0.52, 1.0) };
    let last_words = if t.typed.len() > 54 { &t.typed[t.typed.len() - 54..] } else { &t.typed };
    g.text(if t.typed.is_empty() { "Start typing to begin..." } else { last_words }, Rect::new(27.0, 181.0, 390.0, 18.0), &f.small, if t.typed.is_empty() { T3.fade(ca) } else { input_color.fade(ca) }, Align::Left);
    if t.finished {
        g.text(&format!("Finished! {} WPM · Best: {}", t.wpm(), t.best_wpm), Rect::new(24.0, 199.0, 370.0, 12.0), &f.tiny, T1.fade(ca), Align::Left);
    } else {
        g.text("Backspace to fix · Esc to exit", Rect::new(24.0, 199.0, 330.0, 12.0), &f.tiny, T3.fade(ca), Align::Left);
    }
    let duration = Rect::new(421.0, 181.0, 70.0, 22.0);
    hits.push(Hit { r: duration, act: Act::TypingDuration });
    g.fill_rrect(duration, 10.0, (if hot(a, Act::TypingDuration) { HOVER } else { SURF }).fade(ca));
    g.text(&format!("{}s", t.duration_s), duration, &f.small, T1.fade(ca), Align::Center);
    let retry = Rect::new(494.0, 181.0, 79.0, 22.0);
    hits.push(Hit { r: retry, act: Act::TypingRestart });
    g.fill_rrect(retry, 12.0, (if hot(a, Act::TypingRestart) { HOVER } else { SURF }).fade(ca));
    g.text("New text", retry, &f.tiny, T1.fade(ca), Align::Center);
}

fn page_clipboard(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let view = a.clip_view();

    let total = a.pins.len() + a.clips.len();
    let head = if total == 0 {
        "Clipboard".to_string()
    } else {
        format!("Clipboard  \u{00B7}  {}", total)
    };
    g.text(&head, Rect::new(28.0, 12.0, 200.0, 24.0), &f.small, C_MEM.fade(ca), Align::Left);
    if !a.clips.is_empty() {
        let r = Rect::new(EXP_W - 112.0, 12.0, 64.0, 24.0);
        hits.push(Hit { r, act: Act::ClipClear });
        if hot(a, Act::ClipClear) {
            g.fill_rrect(r, 12.0, Color::white(0.10 * ca));
        }
        g.text("Clear", r, &f.small, C_PINK.fade(ca), Align::Center);
    }
    // search chip: click, type to filter, Enter keeps it, Esc clears it
    let sr = Rect::new(EXP_W - 252.0, 12.0, 132.0, 24.0);
    let typing = a.entry_search;
    if typing || !a.clip_filter.is_empty() {
        let mut t = a.clip_filter.clone();
        if typing {
            t.push('\u{2502}');
        }
        g.fill_rrect(sr, 12.0, Color::white(0.14 * ca));
        g.text(&t, Rect::new(sr.x + 10.0, sr.y, sr.w - 30.0, sr.h), &f.small, T1.fade(ca), Align::Left);
        if !typing {
            let x = Rect::new(sr.x + sr.w - 24.0, sr.y, 24.0, sr.h);
            hits.push(Hit { r: sr, act: Act::ClipSearch });
            hits.push(Hit { r: x, act: Act::ClipSearchClear });
            g.icon('\u{E711}', x, &f.icon14, (if hot(a, Act::ClipSearchClear) { T1 } else { T2 }).fade(ca));
        }
    } else {
        hits.push(Hit { r: sr, act: Act::ClipSearch });
        if hot(a, Act::ClipSearch) {
            g.fill_rrect(sr, 12.0, Color::white(0.10 * ca));
        }
        g.icon('\u{E721}', Rect::new(sr.x + 4.0, sr.y, 24.0, sr.h), &f.icon14, T3.fade(ca));
        g.text("Search", Rect::new(sr.x + 30.0, sr.y, sr.w - 34.0, sr.h), &f.small, T3.fade(ca), Align::Left);
    }

    if view.is_empty() {
        let (l1, l2) = if a.clip_filter.is_empty() {
            ("Copy something and it will appear here.", "Click an item to paste it. Click the star to pin it as a saved snippet.")
        } else {
            ("No matches.", "Press Esc in the search box to clear the filter.")
        };
        g.text(l1, Rect::new(0.0, 80.0, EXP_W, 24.0), &f.body, T3.fade(ca), Align::Center);
        g.text(l2, Rect::new(0.0, 104.0, EXP_W, 20.0), &f.small, T3.fade(ca * 0.8), Align::Center);
        return;
    }

    for row in 0..CLIP_ROWS {
        let idx = a.clip_scroll + row;
        let Some((text, pinned)) = view.get(idx) else { break };
        let r = Rect::new(24.0, 40.0 + row as f32 * 31.0, EXP_W - 48.0, 28.0);
        let act = Act::ClipItem(idx);
        hits.push(Hit { r, act });
        let pin = Rect::new(r.x + r.w - 32.0, r.y, 30.0, r.h);
        let pact = Act::ClipPin(idx);
        hits.push(Hit { r: pin, act: pact });
        let (kind, kc) = clip_kind(text);
        if hot(a, act) || hot(a, pact) {
            g.fill_rrect(r, 10.0, Color::white(0.12 * ca));
        } else {
            g.fill_rrect(r, 10.0, Color::white(0.04 * ca));
        }
        g.fill_rrect(r, 10.0, kc.fade(0.06 * ca));
        // a round chip in the colour of the kind of thing copied
        let chip = Rect::new(r.x + 6.0, r.y + 3.0, 22.0, 22.0);
        g.fill_circle(chip.cx(), chip.cy(), 11.0, kc.fade(0.20 * ca));
        g.icon(kind, chip, &f.icon14, kc.fade(ca));
        g.text(&preview(text), Rect::new(r.x + 38.0, r.y, r.w - 76.0, r.h), &f.body, T1.fade(ca), Align::Left);
        let (glyph, col) = if *pinned {
            ('\u{E735}', C_GOLD)
        } else if hot(a, pact) {
            ('\u{E734}', T1)
        } else {
            ('\u{E734}', T3)
        };
        g.icon(glyph, pin, &f.icon14, col.fade(ca * if *pinned || hot(a, act) || hot(a, pact) { 1.0 } else { 0.6 }));
    }

    // scroll bar: the thumb's size shows how much of the list is on screen, its place where you are
    if view.len() > CLIP_ROWS {
        let (top, h) = (40.0, CLIP_ROWS as f32 * 31.0 - 3.0);
        let track = Rect::new(EXP_W - 17.0, top, 4.0, h);
        g.fill_rrect(track, 2.0, Color::white(0.07 * ca));
        let thumb_h = (h * CLIP_ROWS as f32 / view.len() as f32).max(18.0);
        let frac = a.clip_scroll as f32 / (view.len() - CLIP_ROWS) as f32;
        g.fill_rrect(Rect::new(track.x, top + (h - thumb_h) * frac.clamp(0.0, 1.0), 4.0, thumb_h), 2.0, C_MEM.fade(0.8 * ca));
    }
}

const C_GOLD: Color = Color(1.0, 0.82, 0.28, 1.0);
const C_PINK: Color = Color(1.0, 0.50, 0.62, 1.0);

/// What kind of thing was copied: an icon and a colour for its row.
fn clip_kind(s: &str) -> (char, Color) {
    match clip_kind_of(s) {
        ClipKind::Link => ('\u{E71B}', C_CPU),
        ClipKind::Mail => ('\u{E715}', C_BAT),
        ClipKind::Path => ('\u{E8B7}', C_MEM),
        ClipKind::Code => ('\u{E943}', C_PINK),
        ClipKind::Text => ('\u{E8A5}', C_UP),
    }
}

// ------------------------------------------------------------------ apps ----

fn spinner(g: &Gfx, cx: f32, cy: f32, r: f32, t: f32, col: Color, ca: f32) {
    for i in 0..10 {
        let ang = t * 6.0 - i as f32 * 0.55;
        g.fill_circle(cx + r * ang.cos(), cy + r * ang.sin(), 3.4 - i as f32 * 0.25, col.fade((1.0 - i as f32 / 10.0) * ca));
    }
}

fn page_apps(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    if a.picking {
        page_app_picker(a, ca, hits);
        return;
    }
    let g = &a.gfx;
    let f = &g.fonts;
    let (cols, tw, th) = (6usize, 88.0, 86.0);
    let gap = (EXP_W - 56.0 - cols as f32 * tw) / (cols as f32 - 1.0);
    let cell = |i: usize| {
        let (c, r) = (i % cols, i / cols);
        Rect::new(28.0 + c as f32 * (tw + gap), 6.0 + r as f32 * (th + 8.0), tw, th)
    };
    for (i, e) in a.apps.iter().enumerate().take(12) {
        let rect = cell(i);
        let act = Act::Launch(i);
        hits.push(Hit { r: rect, act });
        let x = Rect::new(rect.x + rect.w - 22.0, rect.y + 3.0, 19.0, 19.0);
        let xact = Act::AppUnpin(i);
        hits.push(Hit { r: x, act: xact });
        let over = hot(a, act) || hot(a, xact);
        let k = hot_k(a, act).max(hot_k(a, xact));
        g.fill_rrect(rect, 16.0, Color::white((0.05 + 0.07 * k) * ca));
        let ic = Rect::new(rect.cx() - 18.0, rect.y + 10.0, 36.0, 36.0);
        match a.app_icons.get(i).and_then(|b| b.as_ref()) {
            Some(bmp) => g.draw_bitmap(bmp, ic, 0.0, ca),
            None => {
                let hue = name_color(&e.name);
                g.fill_circle(ic.cx(), ic.cy(), 18.0, hue.fade(ca));
                let letter: String = e.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                g.text(&letter, ic, &g.fonts.title, Color(0.0, 0.0, 0.0, 0.75 * ca), Align::Center);
            }
        }
        g.text(&e.name, Rect::new(rect.x + 4.0, rect.y + 52.0, rect.w - 8.0, 24.0), &f.tiny, T2.fade(ca), Align::Center);
        if over {
            // unpin
            g.fill_circle(x.cx(), x.cy(), 8.5, Color(0.0, 0.0, 0.0, 0.75 * ca));
            g.icon('\u{E711}', x, &f.icon14, (if hot(a, xact) { Color(1.0, 0.5, 0.5, 1.0) } else { T1 }).fade(ca));
        }
    }
    if a.apps.len() < 12 {
        let rect = cell(a.apps.len());
        hits.push(Hit { r: rect, act: Act::AppAdd });
        let on = hot(a, Act::AppAdd);
        g.fill_rrect(rect, 16.0, Color::white((if on { 0.12 } else { 0.04 }) * ca));
        g.stroke_rrect(rect, 16.0, Color::white((if on { 0.35 } else { 0.14 }) * ca), 1.0);
        g.fill_circle(rect.cx(), rect.y + 28.0, 18.0, a.accent.fade((if on { 0.4 } else { 0.22 }) * ca));
        g.icon('\u{E710}', Rect::new(rect.cx() - 18.0, rect.y + 10.0, 36.0, 36.0), &f.icon14, a.accent.fade(ca));
        g.text("Add app", Rect::new(rect.x + 4.0, rect.y + 52.0, rect.w - 8.0, 24.0), &f.tiny, T2.fade(ca), Align::Center);
    }
}

fn page_shelf(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let area = Rect::new(20.0, 2.0, EXP_W - 40.0, 202.0);
    if a.shelf_hover {
        // a file is being dragged over the notch
        g.fill_rrect(area, 18.0, a.accent.fade(0.14 * ca));
        g.stroke_rrect(area, 18.0, a.accent.fade(0.9 * ca), 2.0);
        g.text("Drop to add to the shelf", area, &f.body, T1.fade(ca), Align::Center);
        return;
    }
    if a.shelf.is_empty() {
        g.stroke_rrect(area, 18.0, Color::white(0.14 * ca), 1.0);
        if a.mascot_visible { draw_miso_at(a, ca, area.cx(), 82.0, false); }
        else { g.icon('\u{E896}', Rect::new(area.cx() - 20.0, 62.0, 40.0, 40.0), &f.icon22, a.accent.fade(ca)); }
        g.text("Drop files here to keep them handy", Rect::new(area.x, 108.0, area.w, 24.0), &f.body, T1.fade(ca), Align::Center);
        g.text("Or copy a file in Explorer, then click below", Rect::new(area.x, 132.0, area.w, 22.0), &f.small, T3.fade(ca), Align::Center);
        let paste = Rect::new(199.0, 161.0, 202.0, 33.0);
        hits.push(Hit { r: paste, act: Act::ShelfPaste });
        g.fill_rrect(paste, 11.0, (if hot(a, Act::ShelfPaste) { a.accent.fade(0.40) } else { HOVER }).fade(ca));
        g.text("Paste copied files", paste, &f.small, T1.fade(ca), Align::Center);
        return;
    }
    // Files remain on the left while their Quick Look stays inside Shelf.
    let (cols, tw, th) = (4usize, 82.0, 51.0);
    let gap = 6.0;
    for (i, path) in a.shelf.iter().enumerate() {
        let (c, r) = (i % cols, i / cols);
        let rect = Rect::new(22.0 + c as f32 * (tw + gap), 6.0 + r as f32 * (th + 5.0), tw, th);
        let act = Act::ShelfPreview(i);
        hits.push(Hit { r: rect, act });
        let x = Rect::new(rect.x + rect.w - 17.0, rect.y + 1.0, 17.0, 17.0);
        let xact = Act::ShelfDel(i);
        hits.push(Hit { r: x, act: xact });
        let selected = a.shelf_preview_path == *path;
        let over = hot(a, act) || hot(a, xact);
        let k = hot_k(a, act).max(hot_k(a, xact));
        g.fill_rrect(rect, 12.0, Color::white((if selected { 0.17 } else { 0.055 + 0.075 * k }) * ca));
        if selected { g.stroke_rrect(rect, 12.0, Color::white(0.34 * ca), 0.9); }
        let ic = Rect::new(rect.x + 5.0, rect.y + 3.0, rect.w - 10.0, 34.0);
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
        let is_image = std::path::Path::new(path).extension().and_then(|e| e.to_str())
            .is_some_and(|e| ["png", "jpg", "jpeg", "bmp", "gif", "webp"].iter().any(|x| e.eq_ignore_ascii_case(x)));
        match a.shelf_icons.get(i).and_then(|b| b.as_ref()) {
            Some(bmp) if is_image => {
                let sz = unsafe { bmp.GetSize() };
                let ratio = if sz.height > 0.0 { sz.width / sz.height } else { 1.0 };
                let (bw, bh) = if ratio > ic.w / ic.h { (ic.w, ic.w / ratio) } else { (ic.h * ratio, ic.h) };
                g.fill_rrect(ic, 7.0, Color::white(0.045 * ca));
                g.draw_bitmap(bmp, Rect::new(ic.cx() - bw / 2.0, ic.cy() - bh / 2.0, bw, bh), 5.0, ca);
            }
            Some(bmp) => g.draw_bitmap(bmp, Rect::new(rect.cx() - 13.0, rect.y + 6.0, 26.0, 26.0), 0.0, ca),
            None => g.icon('\u{E8A5}', Rect::new(rect.cx() - 13.0, rect.y + 6.0, 26.0, 26.0), &f.icon14, T2.fade(ca)),
        }
        g.text(&name, Rect::new(rect.x + 4.0, rect.y + 36.0, rect.w - 8.0, 14.0), &f.tiny, T2.fade(ca), Align::Center);
        if over {
            g.fill_circle(x.cx(), x.cy(), 8.5, Color(0.0, 0.0, 0.0, 0.75 * ca));
            g.icon('\u{E711}', x, &f.icon14, (if hot(a, xact) { Color(1.0, 0.5, 0.5, 1.0) } else { T1 }).fade(ca));
        }
    }
    let pane = Rect::new(381.0, 5.0, 198.0, 169.0);
    g.fill_rrect(pane, 16.0, Color::white(0.065 * ca));
    if a.shelf_preview_path.is_empty() || !a.shelf.iter().any(|p| *p == a.shelf_preview_path) {
        g.icon('\u{E890}', Rect::new(462.0, 54.0, 34.0, 34.0), &f.icon22, T3.fade(ca));
        g.text("Select a file to preview", Rect::new(392.0, 94.0, 176.0, 33.0), &f.small, T2.fade(ca), Align::Center);
    } else {
        let filename = std::path::Path::new(&a.shelf_preview_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        g.text(&filename, Rect::new(393.0, 11.0, 174.0, 18.0), &f.small, T1.fade(ca), Align::Center);
        let image = Rect::new(393.0, 34.0, 174.0, 89.0);
        if let Some(ref bmp) = a.shelf_preview_bitmap {
            let size = unsafe { bmp.GetSize() };
            let ratio = if size.height > 0.0 { size.width / size.height } else { 1.0 };
            let (bw, bh) = if ratio > image.w / image.h { (image.w, image.w / ratio) } else { (image.h * ratio, image.h) };
            g.draw_bitmap(bmp, Rect::new(image.cx() - bw / 2.0, image.cy() - bh / 2.0, bw, bh), 8.0, ca);
        } else if !a.shelf_preview_text.is_empty() {
            for (j, line) in a.shelf_preview_text.iter().take(5).enumerate() {
                g.text(line, Rect::new(400.0, 35.0 + j as f32 * 17.0, 160.0, 17.0), &f.tiny, T2.fade(ca), Align::Left);
            }
        } else {
            g.icon('\u{E8A5}', Rect::new(462.0, 60.0, 32.0, 32.0), &f.icon22, T2.fade(ca));
        }
        g.text(&a.shelf_preview_info, Rect::new(393.0, 122.0, 174.0, 17.0), &f.tiny, T3.fade(ca), Align::Center);
        for (j, (act, label)) in [(Act::ShelfPreviewOpen, "Open"), (Act::ShelfPreviewReveal, "Reveal"), (Act::ShelfPreviewCopyPath, "Path")].iter().enumerate() {
            let button = Rect::new(392.0 + j as f32 * 59.0, 143.0, 56.0, 24.0);
            hits.push(Hit { r: button, act: *act });
            g.fill_rrect(button, 10.0, (if hot(a, *act) { HOVER } else { SURF }).fade(ca));
            g.text(label, button, &f.tiny, T1.fade(ca), Align::Center);
        }
    }
    let status = crate::shelf::archive_status();
    if !status.label.is_empty() {
        let label = if status.busy && status.total_bytes > 0 {
            format!("{}  {}%", status.label, status.done_bytes.saturating_mul(100) / status.total_bytes)
        } else { status.label };
        g.text(&label, Rect::new(183.0, 190.0, 310.0, 18.0), &f.tiny,
            (if status.busy { a.accent } else { T3 }).fade(ca), Align::Center);
    }
    let clear = Rect::new(EXP_W - 24.0 - 78.0, 190.0, 78.0, 18.0);
    hits.push(Hit { r: clear, act: Act::ShelfClear });
    g.text("Clear all", clear, &f.tiny, (if hot(a, Act::ShelfClear) { T1 } else { T3 }).fade(ca), Align::Right);
    let paste = Rect::new(18.0, 188.0, 158.0, 22.0);
    hits.push(Hit { r: paste, act: Act::ShelfPaste });
    g.text("+ Paste copied files", paste, &f.tiny, (if hot(a, Act::ShelfPaste) { a.accent } else { T2 }).fade(ca), Align::Left);
}

fn page_shelf_preview(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    let name = std::path::Path::new(&a.shelf_preview_path)
        .file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    g.text(&name, Rect::new(22.0, 0.0, EXP_W - 44.0, 25.0), &f.body, T1.fade(ca), Align::Center);
    g.text(&a.shelf_preview_info, Rect::new(22.0, 25.0, EXP_W - 44.0, 18.0), &f.tiny, T3.fade(ca), Align::Center);

    let preview = Rect::new(30.0, 46.0, EXP_W - 60.0, 111.0);
    g.fill_rrect(preview, 12.0, SURF.fade(ca));
    if let Some(ref bitmap) = a.shelf_preview_bitmap {
        let size = unsafe { bitmap.GetSize() };
        let ratio = if size.height > 0.0 { size.width / size.height } else { 1.0 };
        let (w, h) = if ratio >= 270.0 / 107.0 { (270.0, 270.0 / ratio) }
            else { (107.0 * ratio, 107.0) };
        let r = Rect::new(preview.cx() - w / 2.0, preview.cy() - h / 2.0, w, h);
        g.draw_bitmap(bitmap, r, 9.0, ca);
    } else if !a.shelf_preview_text.is_empty() {
        for (i, line) in a.shelf_preview_text.iter().take(6).enumerate() {
            g.text(line, Rect::new(preview.x + 12.0, preview.y + 6.0 + i as f32 * 17.0,
                preview.w - 24.0, 16.0), &f.tiny, T2.fade(ca), Align::Left);
        }
    } else {
        g.icon('\u{E8A5}', Rect::new(preview.cx() - 19.0, preview.y + 21.0, 38.0, 38.0), &f.icon22, a.accent.fade(ca));
        g.text("No inline preview. Open in the original app.",
            Rect::new(preview.x + 12.0, preview.y + 70.0, preview.w - 24.0, 23.0),
            &f.small, T3.fade(ca), Align::Center);
    }

    let buttons = [
        (Act::ShelfPreviewOpen, "Open file"),
        (Act::ShelfPreviewReveal, "Show in Explorer"),
        (Act::ShelfPreviewCopyPath, "Copy path"),
    ];
    for (i, (act, label)) in buttons.iter().enumerate() {
        let r = Rect::new(46.0 + i as f32 * 172.0, 170.0, 161.0, 31.0);
        hits.push(Hit { r, act: *act });
        g.fill_rrect(r, 10.0, (if hot(a, *act) { a.accent.fade(0.35) } else { HOVER }).fade(ca));
        g.text(label, r, &f.small, T1.fade(ca), Align::Center);
    }
}

fn page_app_picker(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;
    g.text("Add an app", Rect::new(28.0, 4.0, 160.0, 26.0), &f.body, T1.fade(ca), Align::Left);
    // search chip (click, then type) and Done
    let done = Rect::new(EXP_W - 24.0 - 76.0, 4.0, 76.0, 26.0);
    hits.push(Hit { r: done, act: Act::AppDone });
    g.fill_rrect(done, 13.0, (if hot(a, Act::AppDone) { Color::white(1.0) } else { Color::white(0.92) }).fade(ca));
    g.text("Done", done, &f.small, Color(0.0, 0.0, 0.0, ca), Align::Center);
    let sr = Rect::new(done.x - 12.0 - 210.0, 4.0, 210.0, 26.0);
    let typing = a.entry_search;
    if typing || !a.app_filter.is_empty() {
        let mut t = a.app_filter.clone();
        if typing {
            t.push('\u{2502}');
        }
        g.fill_rrect(sr, 13.0, Color::white(0.14 * ca));
        g.text(&t, Rect::new(sr.x + 12.0, sr.y, sr.w - 16.0, sr.h), &f.small, T1.fade(ca), Align::Left);
    } else {
        g.fill_rrect(sr, 13.0, Color::white((if hot(a, Act::AppSearch) { 0.12 } else { 0.07 }) * ca));
        g.icon('\u{E721}', Rect::new(sr.x + 6.0, sr.y, 24.0, sr.h), &f.icon14, T3.fade(ca));
        g.text("Search apps", Rect::new(sr.x + 34.0, sr.y, sr.w - 40.0, sr.h), &f.small, T3.fade(ca), Align::Left);
    }
    hits.push(Hit { r: sr, act: Act::AppSearch });
    let rf = Rect::new(sr.x - 36.0, 4.0, 26.0, 26.0);
    hits.push(Hit { r: rf, act: Act::AppRefresh });
    g.fill_rrect(rf, 13.0, Color::white((if hot(a, Act::AppRefresh) { 0.14 } else { 0.07 }) * ca));
    let deg = if a.app_loading { a.spin_t * 360.0 } else { 0.0 };
    g.rotated(rf.cx(), rf.cy(), deg, || g.icon('\u{E72C}', rf, &f.icon14, (if a.app_loading { a.accent } else { T2 }).fade(ca)));
    if a.app_loading && a.app_list.is_some() {
        g.text("Updating\u{2026}", Rect::new(130.0, 4.0, 100.0, 26.0), &f.tiny, T3.fade(ca), Align::Left);
    }

    let list = Rect::new(24.0, 40.0, EXP_W - 48.0, PICK_ROWS as f32 * 32.0);
    let Some(_) = &a.app_list else {
        // still asking Windows for the list; after 15 s offer to try again
        if a.app_load_t0.is_some_and(|t| t.elapsed().as_secs() >= 15) {
            g.text("Windows is taking longer than usual to list your apps.", Rect::new(0.0, 84.0, EXP_W, 24.0), &f.body, T2.fade(ca), Align::Center);
            let r = Rect::new(EXP_W / 2.0 - 60.0, 120.0, 120.0, 32.0);
            hits.push(Hit { r, act: Act::AppRetry });
            g.fill_rrect(r, 16.0, Color::white((if hot(a, Act::AppRetry) { 1.0 } else { 0.92 }) * ca));
            g.text("Try again", r, &f.small, Color(0.0, 0.0, 0.0, ca), Align::Center);
        } else {
            spinner(g, EXP_W / 2.0, 100.0, 16.0, a.spin_t, a.accent, ca);
            g.text("Finding your apps\u{2026}", Rect::new(0.0, 126.0, EXP_W, 22.0), &f.small, T2.fade(ca), Align::Center);
        }
        return;
    };
    let view = a.app_view();
    if view.is_empty() {
        g.text(
            if a.app_filter.is_empty() { "No apps found" } else { "No apps match that search" },
            Rect::new(0.0, 90.0, EXP_W, 24.0),
            &f.body,
            T3.fade(ca),
            Align::Center,
        );
        return;
    }
    for row in 0..PICK_ROWS {
        let idx = a.app_scroll + row;
        let Some((name, id)) = view.get(idx) else { break };
        let r = Rect::new(list.x, list.y + row as f32 * 32.0, list.w - 10.0, 28.0);
        let target = format!("shell:AppsFolder\\{id}");
        let pinned = a.apps.iter().any(|p| p.target == target);
        let act = Act::AppPick(idx);
        if !pinned {
            hits.push(Hit { r, act });
        }
        let on = hot(a, act) && !pinned;
        g.fill_rrect(r, 10.0, Color::white((if on { 0.12 } else if row % 2 == 0 { 0.05 } else { 0.0 }) * ca));
        let ic = Rect::new(r.x + 10.0, r.y + 3.0, 22.0, 22.0);
        match a.picker_icons.iter().find(|(k, _)| k == id).and_then(|(_, b)| b.as_ref()) {
            Some(bmp) => g.draw_bitmap(bmp, ic, 0.0, ca),
            None => {
                g.fill_circle(ic.cx(), ic.cy(), 11.0, name_color(name).fade(ca));
                let letter: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                g.text(&letter, ic, &f.tiny, Color(0.0, 0.0, 0.0, 0.75 * ca), Align::Center);
            }
        }
        g.text(name, Rect::new(r.x + 44.0, r.y, r.w - 150.0, r.h), &f.body, (if pinned { T3 } else { T1 }).fade(ca), Align::Left);
        let (label, col) = if pinned { ("\u{2713} Pinned", T3) } else if on { ("Pin to panel", a.accent) } else { ("", T3) };
        g.text(label, Rect::new(r.x + r.w - 130.0, r.y, 120.0, r.h), &f.small, col.fade(ca), Align::Right);
    }
    // a thin scroll bar when there is more than fits
    if view.len() > PICK_ROWS {
        let track = Rect::new(list.x + list.w - 5.0, list.y, 3.0, PICK_ROWS as f32 * 32.0 - 4.0);
        g.fill_rrect(track, 1.5, Color::white(0.08 * ca));
        let hgt = track.h * PICK_ROWS as f32 / view.len() as f32;
        let y = track.y + (track.h - hgt) * a.app_scroll as f32 / (view.len() - PICK_ROWS) as f32;
        g.fill_rrect(Rect::new(track.x, y, 3.0, hgt), 1.5, Color::white(0.3 * ca));
    }
}

// ----------------------------------------------------------------- tasks ----

fn page_tasks(a: &App, ca: f32, hits: &mut Vec<Hit>) {
    let g = &a.gfx;
    let f = &g.fonts;

    // ---- cards
    for (r, c) in [(Rect::new(14.0, 2.0, 208.0, 198.0), C_FOC), (Rect::new(230.0, 2.0, 190.0, 198.0), C_BAT), (Rect::new(428.0, 2.0, 158.0, 198.0), C_UP)] {
        g.fill_rrect(r, 20.0, Color::white(0.05 * ca));
        g.fill_rrect(r, 20.0, c.fade(0.07 * ca));
    }

    // ---- focus timer
    let title = if a.break_mode {
        "Break".to_string()
    } else if let Some(t) = &a.timer_task {
        let short: String = t.chars().take(20).collect();
        format!("Focus \u{00B7} {short}")
    } else {
        "Focus timer".to_string()
    };
    g.text(
        &title,
        Rect::new(28.0, 12.0, 185.0, 22.0),
        &f.small,
        (if a.break_mode { a.accent } else { C_FOC }).fade(ca),
        Align::Left,
    );
    g.text(
        &format!("Today: {} min focus \u{00B7} {} done", a.focus_today, a.done_today),
        Rect::new(28.0, 176.0, 190.0, 16.0),
        &f.tiny,
        T3.fade(ca),
        Align::Left,
    );
    let left = a.timer_remaining_ms();
    let done = !a.timer_running() && left == 0;
    let face = match (&a.entering, a.entry_timer) {
        (Some(buf), true) => format!("{}\u{2502} min", String::from_utf16_lossy(buf)),
        _ => fmt_time(left as f64 / 1000.0),
    };
    g.text(
        &face,
        Rect::new(28.0, 34.0, 175.0, 52.0),
        &f.big,
        (if done && !a.entry_timer { C_FOC } else { T1 }).fade(ca),
        Align::Left,
    );
    if a.entry_timer {
        g.text(
            "Enter to start \u{00B7} Esc to cancel",
            Rect::new(28.0, 76.0, 190.0, 16.0),
            &f.tiny,
            T3.fade(ca),
            Align::Left,
        );
    }
    if done {
        g.text(
            "Time's up",
            Rect::new(110.0, 12.0, 90.0, 22.0),
            &f.small,
            C_FOC.fade(ca),
            Align::Right,
        );
    }
    for (i, mins) in [5u32, 15, 25, 50].iter().enumerate() {
        let r = Rect::new(28.0 + i as f32 * 38.0, 94.0, 34.0, 26.0);
        let act = Act::TimerPreset(*mins);
        hits.push(Hit { r, act });
        let sel = a.timer_total_s == mins * 60;
        let bg = if hot(a, act) {
            HOVER
        } else if sel {
            C_FOC.fade(0.28)
        } else {
            SURF
        };
        g.fill_rrect(r, 13.0, bg.fade(ca));
        g.text(
            &format!("{}m", mins),
            r,
            &f.small,
            (if sel { C_FOC } else { T2 }).fade(ca),
            Align::Center,
        );
    }
    // custom length: type minutes
    let cr = Rect::new(28.0 + 4.0 * 38.0, 94.0, 34.0, 26.0);
    hits.push(Hit { r: cr, act: Act::TimerCustom });
    g.fill_rrect(cr, 13.0, (if hot(a, Act::TimerCustom) || a.entry_timer { HOVER } else { SURF }).fade(ca));
    g.icon('\u{E70F}', cr, &f.icon14, T2.fade(ca));
    let sb = Rect::new(28.0, 130.0, 108.0, 36.0);
    hits.push(Hit {
        r: sb,
        act: Act::TimerToggle,
    });
    g.fill_rrect(
        sb,
        18.0,
        C_FOC.fade((if hot(a, Act::TimerToggle) { 1.0 } else { 0.9 }) * ca),
    );
    g.text(
        if a.timer_running() { "Pause" } else { "Start" },
        sb,
        &f.body,
        Color(0.0, 0.0, 0.0, ca),
        Align::Center,
    );
    let rb = Rect::new(144.0, 130.0, 36.0, 36.0);
    hits.push(Hit {
        r: rb,
        act: Act::TimerReset,
    });
    g.fill_rrect(
        rb,
        18.0,
        (if hot(a, Act::TimerReset) { HOVER } else { SURF }).fade(ca),
    );
    g.icon('\u{E72C}', rb, &f.icon14, T1.fade(ca));

    // ---- to-do
    let tx = 240.0;
    let tw = 170.0;
    g.text(
        "To-do",
        Rect::new(tx, 12.0, 100.0, 22.0),
        &f.small,
        C_BAT.fade(ca),
        Align::Left,
    );
    for (i, t) in a.todos.iter().enumerate() {
        let y = 38.0 + i as f32 * 28.0;
        let row = Rect::new(tx, y, tw, 26.0);
        let cb = Rect::new(tx, y, 26.0, 26.0);
        let del = Rect::new(tx + tw - 24.0, y, 24.0, 26.0);
        let fb = Rect::new(tx + tw - 50.0, y, 24.0, 26.0);
        if !t.done {
            hits.push(Hit { r: fb, act: Act::TodoFocus(i) });
        }
        hits.push(Hit {
            r: cb,
            act: Act::TodoToggle(i),
        });
        hits.push(Hit {
            r: del,
            act: Act::TodoDel(i),
        });
        let over = hot(a, Act::TodoToggle(i)) || hot(a, Act::TodoDel(i)) || hot(a, Act::TodoFocus(i));
        if over {
            g.fill_rrect(row, 9.0, Color::white(0.07 * ca));
        }
        let glyph = if t.done { '\u{E73A}' } else { '\u{E739}' };
        let gc = if t.done { C_BAT } else { T2 };
        g.icon(glyph, cb, &f.icon14, gc.fade(ca));
        g.text(
            &t.text,
            Rect::new(tx + 30.0, y, tw - 30.0 - 26.0, 26.0),
            &f.small,
            (if t.done { T3 } else { T1 }).fade(ca),
            Align::Left,
        );
        if over {
            g.fill_rrect(Rect::new(tx + tw - 52.0, y + 1.0, 52.0, 24.0), 8.0, Color(0.117, 0.117, 0.117, 1.0).fade(ca));
        }
        if !t.done {
            g.icon(
                '\u{E768}',
                fb,
                &f.icon14,
                (if hot(a, Act::TodoFocus(i)) { a.accent } else { T3 }).fade(ca * if over { 1.0 } else { 0.0 }),
            );
        }
        g.icon(
            '\u{E711}',
            del,
            &f.icon14,
            (if hot(a, Act::TodoDel(i)) { T1 } else { T3 }).fade(ca * if over { 1.0 } else { 0.0 }),
        );
    }
    let y = 38.0 + a.todos.len() as f32 * 28.0;
    if let (Some(buf), false) = (&a.entering, a.entry_timer) {
        let r = Rect::new(tx, y, tw, 26.0);
        g.fill_rrect(r, 9.0, Color::white(0.14 * ca));
        let mut s = String::from_utf16_lossy(buf);
        s.push('\u{2502}'); // caret
        g.text(
            &s,
            Rect::new(r.x + 10.0, r.y, r.w - 16.0, r.h),
            &f.small,
            T1.fade(ca),
            Align::Left,
        );
        g.text(
            "Enter to add \u{00B7} Esc to cancel",
            Rect::new(tx, y + 28.0, tw, 18.0),
            &f.tiny,
            T3.fade(ca),
            Align::Left,
        );
    } else if a.todos.len() < MAX_TODO {
        let r = Rect::new(tx, y, tw, 26.0);
        hits.push(Hit {
            r,
            act: Act::TodoAdd,
        });
        if hot(a, Act::TodoAdd) {
            g.fill_rrect(r, 9.0, Color::white(0.07 * ca));
        }
        g.icon(
            '\u{E710}',
            Rect::new(tx, y, 26.0, 26.0),
            &f.icon14,
            C_BAT.fade(ca),
        );
        g.text(
            "Add task",
            Rect::new(tx + 30.0, y, 140.0, 26.0),
            &f.small,
            T3.fade(ca),
            Align::Left,
        );
    }

    // ---- calendar
    draw_calendar(a, ca);
}

fn draw_calendar(a: &App, ca: f32) {
    let g = &a.gfx;
    let f = &g.fonts;
    let (year, month, day, wday, ..) = sys::local_time();
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let x0 = 432.0;
    let cw = 20.0;
    g.text(
        &format!("{} {}", MONTHS[(month as usize - 1).min(11)], year),
        Rect::new(x0 - 4.0, 12.0, cw * 7.0 + 8.0, 22.0),
        &f.small,
        C_UP.fade(ca),
        Align::Center,
    );
    for (i, d) in ["S", "M", "T", "W", "T", "F", "S"].iter().enumerate() {
        g.text(
            d,
            Rect::new(x0 + i as f32 * cw, 36.0, cw, 18.0),
            &f.tiny,
            C_UP.fade(0.55 * ca),
            Align::Center,
        );
    }
    let first = first_weekday(day, wday);
    let dim = days_in_month(year, month);
    for d in 1..=dim {
        let slot = first + d - 1;
        let (c, r) = (slot % 7, slot / 7);
        let cell = Rect::new(x0 + c as f32 * cw, 56.0 + r as f32 * 20.0, cw, 20.0);
        if d == day {
            g.fill_circle(cell.cx(), cell.cy(), 9.5, C_UP.fade(ca));
            g.text(
                &d.to_string(),
                cell,
                &f.tiny,
                Color(0.0, 0.0, 0.0, ca),
                Align::Center,
            );
        } else {
            let col = if c == 0 || c == 6 { T3 } else { T2 };
            g.text(&d.to_string(), cell, &f.tiny, col.fade(ca), Align::Center);
        }
    }
}

pub(crate) use crate::cal::{days_in_month, first_of_month, first_weekday};

// --------------------------------------------------------------- helpers ----

/// A seek line with gentle rhythm. The shape is decorative; the colour split
/// and handle use the media session's actual playback position.
fn wave_seek(a: &App, r: Rect, frac: f32, ca: f32, hits: &mut Vec<Hit>, enabled: bool) {
    let g = &a.gfx;
    let progress = frac.clamp(0.0, 1.0);
    let segments = 48;
    let point = |i: usize| {
        let t = i as f32 / segments as f32;
        let amplitude = (t * 19.0).sin() * (t * 43.0).sin() * 3.1;
        (r.x + r.w * t, r.cy() + amplitude)
    };
    for i in 0..segments {
        let t = (i as f32 + 0.5) / segments as f32;
        let color = if t <= progress { a.accent.fade(0.92 * ca) } else { Color::white(0.27 * ca) };
        g.line(point(i), point(i + 1), 2.5, color);
    }
    if enabled {
        hits.push(Hit { r, act: Act::Seek });
        if hot(a, Act::Seek) || a.seek_frac.is_some() {
            g.fill_circle(r.x + r.w * progress, r.cy(), 5.5, Color::white(ca));
        }
    }
}

fn slider(
    a: &App,
    r: Rect,
    frac: f32,
    act: Act,
    fill: Color,
    ca: f32,
    hits: &mut Vec<Hit>,
    enabled: bool,
) {
    let g = &a.gfx;
    let track = Rect::new(r.x, r.cy() - 3.0, r.w, 6.0);
    g.fill_rrect(track, 3.0, Color::white(0.16 * ca));
    let fw = (r.w * frac.clamp(0.0, 1.0)).max(0.0);
    // progress shows even when the app does not allow seeking; only the knob and clicks need `enabled`
    if fw > 0.5 {
        g.fill_rrect(Rect::new(r.x, track.y, fw, 6.0), 3.0, fill.fade(ca));
    }
    if enabled {
        hits.push(Hit { r, act });
        if hot(a, act) || a.seek_frac.is_some() && act == Act::Seek {
            g.fill_circle(r.x + fw, r.cy(), 7.0, Color::white(ca));
        }
    }
}

pub use crate::cal::fmt_time;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_fraction_is_clamped() {
        assert_eq!(focus_fraction(1500, 1_500_000), 0.0);
        assert!((focus_fraction(1500, 750_000) - 0.5).abs() < 1e-6);
        assert_eq!(focus_fraction(1500, 0), 1.0);
        assert_eq!(focus_fraction(1500, -5), 1.0);
        assert_eq!(focus_fraction(0, 0), 1.0);
    }

    #[test]
    fn previews_collapse_whitespace_and_truncate() {
        assert_eq!(preview("a \n\t b   c"), "a b c");
        assert_eq!(preview(&"x".repeat(500)).chars().count(), 160);
    }
}
