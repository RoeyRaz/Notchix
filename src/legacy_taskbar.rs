//! One-time recovery for builds that replaced Explorer's taskbar.
//! The replacement feature is removed; restore its saved Windows state on
//! the next launch and clear its opt-in marker.

use windows::Win32::Foundation::LPARAM;
use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_SETSTATE, APPBARDATA};

pub fn restore_if_needed() {
    let data_dir = crate::store::data_dir();
    let marker = data_dir.join("taskbar_restore.txt");
    if let Ok(saved) = std::fs::read_to_string(&marker) {
        if let Ok(prior) = saved.trim().parse::<u32>() {
            unsafe {
                let mut data = APPBARDATA {
                    cbSize: std::mem::size_of::<APPBARDATA>() as u32,
                    lParam: LPARAM(prior as isize),
                    ..Default::default()
                };
                SHAppBarMessage(ABM_SETSTATE, &mut data);
            }
        }
        let _ = std::fs::remove_file(marker);
    }
    let _ = std::fs::write(data_dir.join("dock.txt"), "0");
}
