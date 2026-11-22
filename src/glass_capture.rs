//! Experimental live glass source. Capture stays on a worker and retains only
//! a small, blurred island-sized image in memory. No screenshots are saved.
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const WM_GLASS_FRAME: u32 = 0x8000 + 42;
const PAD: i32 = 24;

#[derive(Clone, Copy)]
struct Request {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub pixels: Option<Vec<u8>>,
}

pub struct GlassCapture {
    tx: Sender<Request>,
    rx: Receiver<Frame>,
    pending: bool,
    last_request: Option<Instant>,
    unchanged: u8,
    last_rect: Option<(i32, i32, i32, i32)>,
}

impl GlassCapture {
    pub fn new(hwnd: HWND) -> Self {
        let (tx, requests) = mpsc::channel::<Request>();
        let (frames, rx) = mpsc::channel::<Frame>();
        let hwnd = hwnd.0 as isize;
        std::thread::spawn(move || {
            let mut last_hash = None;
            while let Ok(request) = requests.recv() {
                let result = capture(request).map(|(sw, sh, raw)| {
                    let hash = fingerprint(&raw) ^ ((sw as u64) << 32) ^ (sh as u64);
                    let changed = last_hash != Some(hash);
                    last_hash = Some(hash);
                    if changed {
                        let (w, h, pixels) =
                            process(&raw, sw, sh, request.w as usize, request.h as usize);
                        Frame {
                            w,
                            h,
                            pixels: Some(pixels),
                        }
                    } else {
                        Frame {
                            w: ((request.w + 1) / 2) as u32,
                            h: ((request.h + 1) / 2) as u32,
                            pixels: None,
                        }
                    }
                });
                if let Ok(frame) = result {
                    if frames.send(frame).is_err() {
                        break;
                    }
                    unsafe {
                        let _ = PostMessageW(
                            Some(HWND(hwnd as *mut _)),
                            WM_GLASS_FRAME,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                } else {
                    // An unavailable capture never blocks future attempts.
                    last_hash = None;
                    if frames
                        .send(Frame {
                            w: 0,
                            h: 0,
                            pixels: None,
                        })
                        .is_err()
                    {
                        break;
                    }
                    unsafe {
                        let _ = PostMessageW(
                            Some(HWND(hwnd as *mut _)),
                            WM_GLASS_FRAME,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                }
            }
        });
        Self {
            tx,
            rx,
            pending: false,
            last_request: None,
            unchanged: 0,
            last_rect: None,
        }
    }

    pub fn request(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if self.pending || w < 32 || h < 20 || w > 2000 || h > 1200 {
            return;
        }
        let rect = (x, y, w, h);
        let resized = self.last_rect != Some(rect);
        let interval = if resized {
            60
        } else if self.unchanged >= 2 {
            700
        } else {
            180
        };
        if self
            .last_request
            .is_some_and(|t| t.elapsed() < Duration::from_millis(interval))
        {
            return;
        }
        self.pending = self.tx.send(Request { x, y, w, h }).is_ok();
        self.last_request = Some(Instant::now());
        if self.pending {
            self.last_rect = Some(rect);
        }
    }

    pub fn receive(&mut self) -> Option<Frame> {
        let frame = self.rx.try_recv().ok();
        if let Some(ref received) = frame {
            self.pending = false;
            if received.pixels.is_some() {
                self.unchanged = 0;
            } else {
                self.unchanged = self.unchanged.saturating_add(1);
            }
        }
        frame
    }
}

fn fingerprint(pixels: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for chunk in pixels.chunks_exact(16) {
        for &i in &[0, 1, 2, 8, 9, 10] {
            let byte = chunk[i];
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn capture(req: Request) -> windows::core::Result<(usize, usize, Vec<u8>)> {
    let width = req.w + PAD * 2;
    let height = req.h + PAD * 2;
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = width;
        bmi.bmiHeader.biHeight = -height;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;
        let mut bits = std::ptr::null_mut();
        let result = (|| {
            let dib = CreateDIBSection(Some(screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
            let old = SelectObject(mem, dib.into());
            let copied = BitBlt(
                mem,
                0,
                0,
                width,
                height,
                Some(screen),
                req.x - PAD,
                req.y - PAD,
                SRCCOPY,
            );
            let pixels = if copied.is_ok() {
                let bytes =
                    std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize);
                Some((width as usize, height as usize, bytes.to_vec()))
            } else {
                None
            };
            SelectObject(mem, old);
            let _ = DeleteObject(dib.into());
            copied?;
            Ok::<_, windows::core::Error>(pixels.unwrap())
        })();
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        result
    }
}

fn process(src: &[u8], sw: usize, sh: usize, out_w: usize, out_h: usize) -> (u32, u32, Vec<u8>) {
    // Half resolution keeps the two blur passes cheap; Direct2D scales it
    // smoothly when the island is drawn.
    let w = sw / 2;
    let h = sh / 2;
    let mut small = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let si = ((y * 2) * sw + x * 2) * 4;
            let di = (y * w + x) * 4;
            for c in 0..3 {
                small[di + c] = ((u16::from(src[si + c])
                    + u16::from(src[si + 4 + c])
                    + u16::from(src[si + sw * 4 + c])
                    + u16::from(src[si + sw * 4 + 4 + c]))
                    / 4) as u8;
            }
            small[di + 3] = 255;
        }
    }
    let horizontal = box_blur(&small, w, h, 8, true);
    let blurred = box_blur(&horizontal, w, h, 8, false);
    let ow = out_w.div_ceil(2);
    let oh = out_h.div_ceil(2);
    let pad = (PAD as usize) / 2;
    let mut output = vec![0u8; ow * oh * 4];
    for y in 0..oh {
        for x in 0..ow {
            // A restrained inward displacement bends the image at the rim.
            let edge = x.min(y).min(ow - 1 - x).min(oh - 1 - y) as f32;
            let bend = ((16.0 - edge).max(0.0) / 16.0).powi(2) * 3.0;
            let dx = if x < ow / 2 { bend } else { -bend };
            let dy = if y < oh / 2 { bend } else { -bend };
            let sx = ((x as f32 + dx).round() as usize + pad).min(w - 1);
            let sy = ((y as f32 + dy).round() as usize + pad).min(h - 1);
            let from = (sy * w + sx) * 4;
            let to = (y * ow + x) * 4;
            output[to..to + 4].copy_from_slice(&blurred[from..from + 4]);
        }
    }
    (ow as u32, oh as u32, output)
}

fn box_blur(src: &[u8], w: usize, h: usize, radius: usize, horizontal: bool) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    for line in 0..lines {
        let index = |pos: usize| if horizontal { (line * w + pos) * 4 } else { (pos * w + line) * 4 };
        let mut sum = [0u32; 3];
        let mut count = 0u32;
        for pos in 0..len.min(radius + 1) {
            let i = index(pos);
            for c in 0..3 { sum[c] += u32::from(src[i + c]); }
            count += 1;
        }
        for pos in 0..len {
            let i = index(pos);
            for c in 0..3 { out[i + c] = (sum[c] / count) as u8; }
            out[i + 3] = 255;
            if pos >= radius {
                let old = index(pos - radius);
                for c in 0..3 { sum[c] -= u32::from(src[old + c]); }
                count -= 1;
            }
            if let Some(next) = pos.checked_add(radius + 1).filter(|&next| next < len) {
                let new = index(next);
                for c in 0..3 { sum[c] += u32::from(src[new + c]); }
                count += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sliding_blur_matches_direct_average_at_edges() {
        let mut src = vec![0u8; 7 * 5 * 4];
        for (i, pixel) in src.chunks_exact_mut(4).enumerate() {
            pixel[0] = (i * 7) as u8;
            pixel[1] = (i * 3) as u8;
            pixel[2] = i as u8;
        }
        for horizontal in [true, false] {
            let out = box_blur(&src, 7, 5, 2, horizontal);
            for y in 0..5usize {
                for x in 0..7usize {
                    let neighbors: Vec<_> = (0..5usize).flat_map(|yy| (0..7usize).map(move |xx| (xx, yy)))
                        .filter(|&(xx, yy)| if horizontal { yy == y && xx.abs_diff(x) <= 2 } else { xx == x && yy.abs_diff(y) <= 2 })
                        .collect();
                    for c in 0..3 {
                        let expected = neighbors.iter().map(|&(xx, yy)| u32::from(src[(yy * 7 + xx) * 4 + c])).sum::<u32>() / neighbors.len() as u32;
                        assert_eq!(u32::from(out[(y * 7 + x) * 4 + c]), expected);
                    }
                }
            }
        }
    }
    #[test]
    fn blur_softens_a_stripe_without_changing_dimensions() {
        let mut src = vec![0u8; 64 * 64 * 4];
        for y in 0..64 {
            for x in 24..40 {
                src[(y * 64 + x) * 4] = 255;
            }
        }
        let (w, h, out) = process(&src, 64, 64, 16, 16);
        assert_eq!((w, h, out.len()), (8, 8, 256));
        assert!(out.iter().step_by(4).any(|&v| v > 0 && v < 255));
    }
}
