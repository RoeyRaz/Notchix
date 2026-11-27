//! On-demand webcam preview. The camera is opened only while the Mirror panel
//! is active, and the WinRT capture objects never cross the UI thread.

use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static GENERATION: AtomicU64 = AtomicU64::new(0);
static STATE: OnceLock<Mutex<Frame>> = OnceLock::new();

#[derive(Clone, Default)]
pub struct Frame {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub status: String,
}

fn state() -> &'static Mutex<Frame> {
    STATE.get_or_init(|| Mutex::new(Frame::default()))
}

fn set_status(message: &str) {
    if let Ok(mut s) = state().lock() {
        if s.status != message {
            s.status = message.into();
            s.seq = s.seq.wrapping_add(1);
        }
    }
}

pub fn latest_if_new(seq: u64) -> Option<Frame> {
    let guard = state().lock().ok()?;
    (guard.seq != seq).then(|| guard.clone())
}

pub fn status() -> String {
    state().lock().map(|s| s.status.clone()).unwrap_or_default()
}

pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut s) = state().lock() {
        s.pixels.clear();
        s.status = "Camera off".into();
        s.seq = s.seq.wrapping_add(1);
    }
}

pub fn start() {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if let Ok(mut s) = state().lock() {
        s.pixels.clear();
        s.status = "Connecting to webcam...".into();
        s.seq = s.seq.wrapping_add(1);
    }
    std::thread::spawn(move || {
        let result = run(generation);
        if let Err(err) = result {
            if GENERATION.load(Ordering::SeqCst) == generation {
                if let Ok(mut s) = state().lock() {
                    s.status = format!("Camera unavailable: {err}");
                    s.pixels.clear();
                    s.seq = s.seq.wrapping_add(1);
                }
            }
        }
    });
}

fn run(generation: u64) -> windows::core::Result<()> {
    use windows::Media::Capture::{MediaCapture, MediaCaptureInitializationSettings, StreamingCaptureMode};
    use windows::Media::Capture::Frames::{MediaFrameSourceGroup, MediaFrameSourceKind, MediaFrameReaderStartStatus};
    use windows::Media::MediaProperties::MediaEncodingSubtypes;
    use windows::Foundation::TypedEventHandler;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()?; }
    let capture_result = (|| -> windows::core::Result<()> {
        // A plain MediaCapture.StartPreviewAsync requires a preview sink, which
        // our Direct2D layered window deliberately does not provide. Use the
        // frame-reader pipeline instead: no hidden XAML preview control.
        set_status("Looking for a color webcam...");
        let groups = MediaFrameSourceGroup::FindAllAsync()?.join()?;
        let mut choice = None;
        for i in 0..groups.Size()? {
            let group = groups.GetAt(i)?;
            let infos = group.SourceInfos()?;
            for j in 0..infos.Size()? {
                let info = infos.GetAt(j)?;
                if info.SourceKind()? == MediaFrameSourceKind::Color {
                    choice = Some((group.clone(), info.Id()?));
                    break;
                }
            }
            if choice.is_some() { break; }
        }
        let (group, source_id) = choice.ok_or_else(|| windows::core::Error::from(windows::core::HRESULT(0x80070490u32 as i32)))?;
        let camera = MediaCapture::new()?;
        let settings = MediaCaptureInitializationSettings::new()?;
        settings.SetStreamingCaptureMode(StreamingCaptureMode::Video)?;
        settings.SetSourceGroup(&group)?;
        camera.InitializeWithSettingsAsync(&settings)?.join()?;
        if GENERATION.load(Ordering::SeqCst) != generation { return Ok(()); }
        set_status("Webcam connected. Starting frame reader...");
        let source = camera.FrameSources()?.Lookup(&source_id)?;
        let subtype = MediaEncodingSubtypes::Bgra8()?;
        let reader = camera.CreateFrameReaderWithSubtypeAsync(&source, &subtype)?.join()?;
        // FrameReader's realtime buffer is populated for FrameArrived
        // handlers. Polling TryAcquireLatestFrame from an unrelated thread
        // can repeatedly return no frame even when the camera is streaming.
        let last = std::sync::Arc::new(Mutex::new(std::time::Instant::now() - Duration::from_secs(1)));
        let callback_reader = reader.clone();
        let callback_last = last.clone();
        let token = reader.FrameArrived(&TypedEventHandler::new(move |_, _| {
            if GENERATION.load(Ordering::SeqCst) != generation { return Ok(()); }
            if let Ok(mut last) = callback_last.try_lock() {
                if last.elapsed() < Duration::from_millis(120) { return Ok(()); }
                *last = std::time::Instant::now();
            } else { return Ok(()); }
            if let Err(err) = read_frame(&callback_reader, generation) {
                if GENERATION.load(Ordering::SeqCst) == generation {
                    set_status(&format!("Camera frame error: {err}"));
                }
            }
            Ok(())
        }))?;
        let started = reader.StartAsync()?.join()?;
        if started != MediaFrameReaderStartStatus::Success {
            let _ = reader.RemoveFrameArrived(token);
            return Err(windows::core::Error::from(windows::core::HRESULT(0x80004005u32 as i32)));
        }
        set_status("Waiting for camera frames...");
        let began = std::time::Instant::now();
        while GENERATION.load(Ordering::SeqCst) == generation {
            if began.elapsed() > Duration::from_secs(6) && status() == "Waiting for camera frames..." {
                set_status("No frames received. Check camera privacy or another app using it.");
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let _ = reader.RemoveFrameArrived(token);
        let _ = reader.StopAsync().and_then(|op| op.join());
        let _ = reader.Close();
        Ok(())
    })();
    unsafe { CoUninitialize(); }
    capture_result
}

fn read_frame(reader: &windows::Media::Capture::Frames::MediaFrameReader, generation: u64) -> windows::core::Result<()> {
    use windows::core::Interface;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Storage::Streams::{Buffer, DataReader};
    // The frame reader can signal FrameArrived before a usable video frame
    // exists. WinRT represents these optional objects as null interfaces.
    // Calling a method on one misleadingly reports HRESULT 0 (success) as an
    // error, which previously appeared as "Camera frame error: ... 0x00000000".
    let frame = reader.TryAcquireLatestFrame()?;
    if frame.as_raw().is_null() { return Ok(()); }
    let video = frame.VideoMediaFrame()?;
    if video.as_raw().is_null() { return Ok(()); }
    let bitmap = video.SoftwareBitmap()?;
    if bitmap.as_raw().is_null() { return Ok(()); }
    let bitmap = if bitmap.BitmapPixelFormat()? == BitmapPixelFormat::Bgra8 {
        bitmap
    } else {
        SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8)?
    };
    let w = bitmap.PixelWidth()?;
    let h = bitmap.PixelHeight()?;
    if w <= 0 || h <= 0 || (w as u64) * (h as u64) > 8_294_400 { return Ok(()); }
    let capacity = (w * h * 4) as u32;
    let buffer = Buffer::Create(capacity)?;
    buffer.SetLength(capacity)?;
    bitmap.CopyToBuffer(&buffer)?;
    let mut pixels = vec![0u8; capacity as usize];
    let data = DataReader::FromBuffer(&buffer)?;
    data.ReadBytes(&mut pixels)?;
    let _ = data.Close();
    if GENERATION.load(Ordering::SeqCst) == generation {
        if let Ok(mut s) = state().lock() {
            s.seq = s.seq.wrapping_add(1);
            s.width = w as u32;
            s.height = h as u32;
            s.pixels = pixels;
            s.status = "Live camera".into();
        }
    }
    Ok(())
}
