//! Windows 10/11 notification-center bridge. Requires an MSIX package identity
//! and the userNotificationListener capability plus explicit user consent.
//! Never captures messages via accessibility, window scraping or clipboard.
use std::collections::{HashSet, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;
use std::time::Instant;
use windows_future::{AsyncStatus, IAsyncOperation};
use windows::Foundation::TypedEventHandler;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::UI::Notifications::{KnownNotificationBindings, NotificationKinds, UserNotification, UserNotificationChangedEventArgs, UserNotificationChangedKind};
use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus};

#[derive(Clone)]
pub struct Notice {
    pub app: String,
    pub title: String,
    pub body: String,
    pub received: Instant,
    /// A toast already in Windows Action Center when Nock first connected.
    /// It belongs in Activity history but must not impersonate a new alert.
    pub historical: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NoticeKind { Regular, Call, DownloadComplete, ExportComplete }

impl NoticeKind {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Regular => "",
            Self::Call => "CALL",
            Self::DownloadComplete => "DOWNLOADED",
            Self::ExportComplete => "EXPORTED",
        }
    }

    pub fn preview_seconds(self) -> u64 { if self == Self::Call { 8 } else { 5 } }
}

/// Browser notifications are branded only when the toast explicitly identifies
/// its site. Message contents alone cannot establish which site sent a toast.
pub fn display_source<'a>(app: &'a str, title: &str, body: &str) -> &'a str {
    let lower = app.to_lowercase();
    if lower.contains("whatsapp") { return "WhatsApp"; }
    if lower.contains("discord") { return "Discord"; }
    let browser = ["chrome", "edge", "firefox", "brave", "opera", "zen"].iter().any(|name| lower.contains(name));
    if browser {
        let text = format!("{} {}", title.to_lowercase(), body.to_lowercase());
        if text.contains("web.whatsapp.com") { return "WhatsApp Web"; }
        if text.contains("discord.com") { return "Discord Web"; }
    }
    app
}

pub fn classify(app: &str, title: &str, body: &str) -> NoticeKind {
    let source = display_source(app, title, body);
    let social = source.starts_with("WhatsApp") || source.starts_with("Discord");
    let status = |phrases: &[&str]| [title, body].iter().any(|line| {
        let text = line.trim().to_lowercase();
        phrases.iter().any(|phrase| text.starts_with(phrase))
    });
    if social && status(&[
        "incoming call", "incoming voice call", "incoming video call", "missed call",
        "שיחה נכנסת", "שיחת וידאו נכנסת", "שיחה שלא נענתה",
    ]) { return NoticeKind::Call; }
    if social && status(&[
        "download complete", "download finished", "file downloaded",
        "ההורדה הושלמה", "הורדה הושלמה",
    ]) { return NoticeKind::DownloadComplete; }
    if status(&[
        "export complete", "export finished", "render complete", "render finished",
        "encoding complete", "ייצוא הושלם", "הרינדור הושלם",
    ]) { return NoticeKind::ExportComplete; }
    NoticeKind::Regular
}

#[cfg(test)]
mod notice_kind_tests {
    use super::{classify, display_source, NoticeKind};

    #[test]
    fn classifies_explicit_social_events_without_promoting_chat_text() {
        assert_eq!(classify("WhatsApp", "Alice", "Incoming video call"), NoticeKind::Call);
        assert_eq!(classify("Discord", "Missed call", "From Alice"), NoticeKind::Call);
        assert_eq!(classify("WhatsApp", "Alice", "Can you call me later?"), NoticeKind::Regular);
        assert_eq!(classify("Discord", "Alice", "Download this file"), NoticeKind::Regular);
        assert_eq!(classify("WhatsApp", "Download complete", "photo.jpg"), NoticeKind::DownloadComplete);
        assert_eq!(classify("Media Encoder", "Export complete", "Project"), NoticeKind::ExportComplete);
    }

    #[test]
    fn browser_toast_needs_explicit_site_identity() {
        assert_eq!(display_source("Google Chrome", "web.whatsapp.com", "Incoming call"), "WhatsApp Web");
        assert_eq!(classify("Google Chrome", "web.whatsapp.com", "Incoming call"), NoticeKind::Call);
        assert_eq!(classify("Google Chrome", "Alice", "Incoming call"), NoticeKind::Regular);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Permission { Unavailable, Unspecified, Pending, Denied, Allowed }

pub struct Listener {
    request: Option<IAsyncOperation<UserNotificationListenerAccessStatus>>,
    pub permission: Permission,
}

impl Listener {
    pub fn new(hwnd: HWND) -> Self {
        UI_HWND.store(hwnd.0 as isize, Ordering::Release);
        let permission = access_status();
        // The listener must not depend on WM_TIMER delivery. Animated layered
        // windows can keep the UI message queue busy, starving low-priority
        // timer messages. Start the worker as soon as consent is already valid.
        if permission == Permission::Allowed { start_poll(); }
        Self { request: None, permission }
    }

    /// Call on the app's UI thread, where Windows can display its consent UI.
    pub fn request(&mut self) {
        if self.request.is_some() { return; }
        match UserNotificationListener::Current().and_then(|l| l.RequestAccessAsync()) {
            Ok(op) => { self.request = Some(op); self.permission = Permission::Pending; }
            Err(_) => { self.permission = Permission::Unavailable; }
        }
    }

    pub fn tick(&mut self) {
        if let Some(op) = &self.request {
            match op.Status() {
                Ok(AsyncStatus::Completed) => {
                    self.permission = match op.GetResults() {
                        Ok(UserNotificationListenerAccessStatus::Allowed) => Permission::Allowed,
                        Ok(_) => Permission::Denied,
                        Err(_) => Permission::Unavailable,
                    };
                    self.request = None;
                }
                Ok(AsyncStatus::Started) => return,
                _ => { self.permission = Permission::Unavailable; self.request = None; }
            }
        } else {
            self.permission = access_status();
        }
        if self.permission == Permission::Allowed { start_poll(); }
    }
}

fn access_status() -> Permission {
    match UserNotificationListener::Current().and_then(|l| l.GetAccessStatus()) {
        Ok(UserNotificationListenerAccessStatus::Allowed) => Permission::Allowed,
        Ok(UserNotificationListenerAccessStatus::Denied) => Permission::Denied,
        Ok(_) => Permission::Unspecified,
        Err(_) => Permission::Unavailable,
    }
}

struct Inbox {
    seen: HashSet<(String, u32)>,
    initialized: bool,
    pending: VecDeque<Notice>,
    detail: String,
}
static INBOX: OnceLock<Mutex<Inbox>> = OnceLock::new();
static BUSY: AtomicBool = AtomicBool::new(false);
static UI_HWND: AtomicIsize = AtomicIsize::new(0);
fn inbox() -> &'static Mutex<Inbox> {
    INBOX.get_or_init(|| Mutex::new(Inbox { seen: HashSet::new(), initialized: false, pending: VecDeque::new(), detail: "Awaiting listener permission".into() }))
}

/// One persistent worker owns Windows' event subscription independently of the
/// notch animation loop and low-priority WM_TIMER messages.
fn start_poll() {
    if BUSY.swap(true, Ordering::AcqRel) { return; }
    std::thread::spawn(|| {
        if let Err(e) = event_worker() {
            if let Ok(mut state) = inbox().lock() {
                state.detail = format!("Windows notification event listener failed: {e}");
            }
            BUSY.store(false, Ordering::Release);
        }
    });
}

fn event_worker() -> windows::core::Result<()> {
    // This dedicated worker is separate from Nock's STA UI thread.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()?; }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) { unsafe { CoUninitialize(); } }
    }
    let _apartment = Apartment;
    let listener = UserNotificationListener::Current()?;
    if listener.GetAccessStatus()? != UserNotificationListenerAccessStatus::Allowed {
        if let Ok(mut state) = inbox().lock() {
            state.detail = "Windows notification permission has been revoked".into();
        }
        return Ok(());
    }
    let (tx, rx) = mpsc::channel::<(u32, bool)>();
    let handler = TypedEventHandler::<UserNotificationListener, UserNotificationChangedEventArgs>::new(move |_sender, args| {
        if let Some(args) = args.as_ref() {
            let added = args.ChangeKind()? == UserNotificationChangedKind::Added;
            let id = args.UserNotificationId()?;
            let _ = tx.send((id, added));
        }
        Ok(())
    });
    let token = listener.NotificationChanged(&handler)?;
    if let Ok(mut state) = inbox().lock() {
        state.detail = "Live Windows notification events connected · loading history".into();
    }
    // Snapshot existing toasts once, for Activity history only. Windows may
    // occasionally stall the snapshot API, but that must never stall or stop
    // the new-notification event subscription.
    if let Err(e) = load_initial_history(&listener) {
        if let Ok(mut state) = inbox().lock() {
            state.detail = format!("Live alerts connected · history unavailable: {e}");
        }
    }
    loop {
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok((id, true)) => {
                // The event may precede the toast becoming readable. Retry
                // briefly instead of silently dropping a just-arrived alert.
                for attempt in 0..4 {
                    if let Ok(item) = listener.GetNotification(id) {
                        if let Ok(Some((app, id, title, body))) = parse_notice(&item) {
                            enqueue(app, id, title, body, false);
                            break;
                        }
                    }
                    if attempt != 3 { std::thread::sleep(Duration::from_millis(120)); }
                }
            }
            Ok((id, false)) => {
                if let Ok(mut state) = inbox().lock() {
                    state.seen.retain(|(_, seen_id)| *seen_id != id);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if listener.GetAccessStatus()? != UserNotificationListenerAccessStatus::Allowed {
                    if let Ok(mut state) = inbox().lock() {
                        state.detail = "Windows notification permission was revoked".into();
                    }
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    listener.RemoveNotificationChanged(token)?;
    Ok(())
}

fn load_initial_history(listener: &UserNotificationListener) -> windows::core::Result<()> {
    let operation = listener.GetNotificationsAsync(NotificationKinds::Toast)?;
    let mut ready = false;
    for _ in 0..70 {
        match operation.Status()? {
            AsyncStatus::Completed => { ready = true; break; }
            AsyncStatus::Started => std::thread::sleep(Duration::from_millis(100)),
            status => {
                if let Ok(mut state) = inbox().lock() {
                    state.detail = format!("Windows notification request stopped: {status:?}");
                }
                return Ok(());
            }
        }
    }
    if !ready {
        if let Ok(mut state) = inbox().lock() {
            state.detail = "Windows notification request timed out after 7 seconds".into();
        }
        return Ok(());
    }
    let items = operation.GetResults()?;
    let mut current = Vec::new();
    for i in 0..items.Size()?.min(100) {
        let item = match items.GetAt(i) { Ok(item) => item, Err(_) => continue };
        if let Ok(Some(notice)) = parse_notice(&item) { current.push(notice); }
    }
    let mut guard = inbox().lock().unwrap();
    let mut next_seen = HashSet::new();
    let mut added = 0usize;
    for (app, id, title, body) in current {
        let key = (app.clone(), id);
        if !guard.seen.contains(&key) {
            let historical = !guard.initialized;
            guard.pending.push_back(Notice { app, title, body, received: Instant::now(), historical });
            added += 1;
        }
        next_seen.insert(key);
    }
    guard.seen = next_seen;
    guard.initialized = true;
    guard.detail = format!("Listening · {} Windows toasts in notification center", guard.seen.len());
    while guard.pending.len() > 50 { guard.pending.pop_front(); }
    drop(guard);
    // WM_APP delivery has normal queue priority, unlike WM_TIMER. It wakes
    // the notch immediately even while the animation pump is active.
    if added > 0 {
        let raw = UI_HWND.load(Ordering::Acquire);
        if raw != 0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(raw as *mut _)), crate::app::WM_NATIVE_NOTICE, WPARAM(0), LPARAM(0));
            }
        }
    }
    Ok(())
}

fn parse_notice(item: &UserNotification) -> windows::core::Result<Option<(String, u32, String, String)>> {
    let app = item.AppInfo()?.DisplayInfo()?.DisplayName()?.to_string();
    let id = item.Id()?;
    let binding = item.Notification()?.Visual()?.GetBinding(&KnownNotificationBindings::ToastGeneric()?)?;
    let elements = binding.GetTextElements()?;
    let mut lines = Vec::new();
    for j in 0..elements.Size()?.min(4) {
        let value = elements.GetAt(j)?.Text()?.to_string();
        if !value.trim().is_empty() { lines.push(value); }
    }
    if lines.is_empty() { return Ok(None); }
    let title = lines.remove(0);
    Ok(Some((app, id, title, lines.join(" · "))))
}

fn enqueue(app: String, id: u32, title: String, body: String, historical: bool) {
    let mut state = inbox().lock().unwrap();
    if !state.seen.insert((app.clone(), id)) { return; }
    state.pending.push_back(Notice { app, title, body, received: Instant::now(), historical });
    state.detail = format!("Live notifications connected · {} tracked", state.seen.len());
    while state.pending.len() > 50 { state.pending.pop_front(); }
    drop(state);
    let raw = UI_HWND.load(Ordering::Acquire);
    if raw != 0 {
        unsafe { let _ = PostMessageW(Some(HWND(raw as *mut _)), crate::app::WM_NATIVE_NOTICE, WPARAM(0), LPARAM(0)); }
    }
}

pub fn take_new() -> Vec<Notice> {
    let mut guard = inbox().lock().unwrap();
    guard.pending.drain(..).collect()
}

pub fn diagnostic() -> String {
    inbox().lock().map(|s| s.detail.clone()).unwrap_or_else(|_| "Listener busy".into())
}
