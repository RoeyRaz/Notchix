//! File shelf plumbing: files dragged onto the notch arrive through an OLE drop
//! target, and a shelf file is dragged back out with a shell data object.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
pub struct ArchiveStatus {
    pub label: String,
    pub busy: bool,
    pub done_bytes: u64,
    pub total_bytes: u64,
}
static ARCHIVE_STATUS: OnceLock<Mutex<ArchiveStatus>> = OnceLock::new();

pub fn archive_status() -> ArchiveStatus {
    ARCHIVE_STATUS.get_or_init(Default::default).lock().map(|s| s.clone()).unwrap_or_default()
}

fn set_archive_status(label: String, busy: bool) {
    if let Ok(mut s) = ARCHIVE_STATUS.get_or_init(Default::default).lock() {
        *s = ArchiveStatus { label, busy, done_bytes: 0, total_bytes: 0 };
    }
}

fn set_copy_progress(done: u64, total: u64) {
    if let Ok(mut s) = ARCHIVE_STATUS.get_or_init(Default::default).lock() {
        s.done_bytes = done;
        s.total_bytes = total;
    }
}

fn bytes_in(path: &Path) -> std::io::Result<u64> {
    if path.is_file() { return Ok(path.metadata()?.len()); }
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        total = total.saturating_add(bytes_in(&entry?.path())?);
    }
    Ok(total)
}

fn copy_with_progress(source: &Path, dest: &Path, done: &mut u64, total: u64) -> std::io::Result<()> {
    if source.is_dir() {
        std::fs::create_dir(dest)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_with_progress(&entry.path(), &dest.join(entry.file_name()), done, total)?;
        }
    } else {
        use std::io::{Read, Write};
        let mut input = std::fs::File::open(source)?;
        let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(dest)?;
        let mut buf = [0u8; 128 * 1024];
        loop {
            let count = input.read(&mut buf)?;
            if count == 0 { break; }
            output.write_all(&buf[..count])?;
            *done = done.saturating_add(count as u64);
            set_copy_progress(*done, total);
        }
        output.flush()?;
    }
    Ok(())
}

/// Copy a Shelf file/folder to a user-selected folder and report actual bytes
/// transferred. The original is never moved or overwritten.
pub fn copy_in_background(source: String) {
    if archive_status().busy { return; }
    let source = PathBuf::from(source);
    if !source.exists() { return; }
    set_archive_status("Choose a destination folder...".into(), true);
    std::thread::spawn(move || {
        let choice = crate::sys::powershell(
            "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = 'Copy Shelf item to...'; if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { Write-Output $d.SelectedPath }"
        );
        let Some(folder) = choice.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()) else {
            set_archive_status("Copy cancelled".into(), false);
            return;
        };
        let result = (|| -> std::io::Result<PathBuf> {
            let Some(name) = source.file_name() else { return Err(std::io::Error::other("invalid filename")); };
            let dest_root = PathBuf::from(folder);
            if !dest_root.is_dir() { return Err(std::io::Error::other("destination not found")); }
            let mut dest = dest_root.join(name);
            if dest.exists() {
                let stem = source.file_stem().unwrap_or(name).to_string_lossy();
                let extension = source.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                let mut free = None;
                for n in 1..1000 {
                    let candidate = dest_root.join(format!("{stem} copy ({n}){extension}"));
                    if !candidate.exists() { free = Some(candidate); break; }
                }
                dest = free.ok_or_else(|| std::io::Error::other("no free destination"))?;
            }
            // A directory cannot be copied into itself or a descendant.
            if source.is_dir() && dest.starts_with(&source) { return Err(std::io::Error::other("destination is inside source")); }
            let total = bytes_in(&source)?;
            set_archive_status(format!("Copying {}", name.to_string_lossy()), true);
            set_copy_progress(0, total);
            let mut done = 0;
            copy_with_progress(&source, &dest, &mut done, total)?;
            Ok(dest)
        })();
        match result {
            Ok(dest) => set_archive_status(format!("Copied {}", dest.file_name().unwrap_or_default().to_string_lossy()), false),
            Err(err) => set_archive_status(format!("Copy failed: {err}"), false),
        }
    });
}

fn unused_sibling(path: &Path, suffix: &str) -> Option<PathBuf> {
    let parent = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy();
    for n in 0..1000 {
        let name = if n == 0 { format!("{stem}{suffix}") } else { format!("{stem} ({n}){suffix}") };
        let candidate = parent.join(name);
        if !candidate.exists() { return Some(candidate); }
    }
    None
}

/// Archive operations are explicitly started from the Shelf. Never overwrite a
/// pre-existing archive or extraction directory. Work off the UI thread.
pub fn archive_in_background(source: String, extract: bool) {
    if archive_status().busy { return; }
    let path = PathBuf::from(source);
    if !path.exists() { return; }
    if extract && !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")) { return; }
    let verb = if extract { "Extracting" } else { "Compressing" };
    set_archive_status(format!("{verb} {}...", path.file_name().unwrap_or_default().to_string_lossy()), true);
    std::thread::spawn(move || {
        let done = (|| -> std::io::Result<PathBuf> {
            let Some(parent) = path.parent() else { return Err(std::io::Error::other("missing parent")); };
            if extract {
                let Some(dest) = unused_sibling(&path, " extracted") else { return Err(std::io::Error::other("no free destination")); };
                std::fs::create_dir(&dest)?;
                let result = std::process::Command::new("tar.exe").arg("-xf").arg(&path).arg("-C").arg(&dest).status()?;
                if !result.success() { return Err(std::io::Error::other("tar extraction failed")); }
                Ok(dest)
            } else {
                let Some(dest) = unused_sibling(&path, ".zip") else { return Err(std::io::Error::other("no free destination")); };
                let Some(name) = path.file_name() else { return Err(std::io::Error::other("missing filename")); };
                let result = std::process::Command::new("tar.exe").arg("-a").arg("-cf").arg(&dest).arg("-C").arg(parent).arg(name).status()?;
                if !result.success() { return Err(std::io::Error::other("tar compression failed")); }
                Ok(dest)
            }
        })();
        match done {
            Ok(dest) => set_archive_status(format!("Saved {}", dest.file_name().unwrap_or_default().to_string_lossy()), false),
            Err(err) => set_archive_status(format!("Archive failed: {err}"), false),
        }
    });
}

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::*;

/// The drag in progress carries files, so the notch accepts it.
static ACCEPT: AtomicBool = AtomicBool::new(false);

fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// Full paths of the files inside a drop.
fn paths_of(data: &IDataObject) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let Ok(mut med) = data.GetData(&hdrop_format()) else { return out };
        let hdrop = HDROP(med.u.hGlobal.0);
        let n = DragQueryFileW(hdrop, u32::MAX, None);
        for i in 0..n {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            out.push(String::from_utf16_lossy(&buf[..len]));
        }
        ReleaseStgMedium(&mut med);
    }
    out
}

/// Explorer's Copy action places CF_HDROP file paths on the clipboard.
/// This is a useful fallback if Windows won't deliver OLE drag-and-drop.
pub fn clipboard_paths() -> Vec<String> {
    unsafe { OleGetClipboard().ok().map(|data| paths_of(&data)).unwrap_or_default() }
}

/// Legacy shell drops (WM_DROPFILES) provide a second route into the Shelf.
pub fn paths_from_hdrop(drop: HDROP) -> Vec<String> {
    let mut paths = Vec::new();
    unsafe {
        let n = DragQueryFileW(drop, u32::MAX, None);
        for i in 0..n {
            let len = DragQueryFileW(drop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(drop, i, Some(&mut buf));
            paths.push(String::from_utf16_lossy(&buf[..len]));
        }
        DragFinish(drop);
    }
    paths
}

#[implement(IDropTarget)]
struct Target;

#[allow(non_snake_case)]
impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, pdataobj: Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let files = pdataobj.as_ref().is_some_and(|d| unsafe { d.QueryGetData(&hdrop_format()) } == S_OK);
        // our own drag-out passing over the notch is not a new drop
        let ours = crate::app().is_some_and(|a| a.shelf_dragging());
        let ok = files && !ours;
        ACCEPT.store(ok, Ordering::Relaxed);
        unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if ok {
            if let Some(a) = crate::app() {
                a.on_shelf_enter();
            }
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *effect = if ACCEPT.load(Ordering::Relaxed) { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        if ACCEPT.swap(false, Ordering::Relaxed) {
            if let Some(a) = crate::app() {
                a.on_shelf_leave();
            }
        }
        Ok(())
    }

    fn Drop(&self, pdataobj: Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let accepted = ACCEPT.swap(false, Ordering::Relaxed);
        let paths = pdataobj.as_ref().map(paths_of).unwrap_or_default();
        unsafe { *effect = if accepted && !paths.is_empty() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if let Some(a) = crate::app() {
            a.on_shelf_drop(paths);
        }
        Ok(())
    }
}

/// Let the notch window receive dropped files. Needs OleInitialize on this thread.
pub fn register(hwnd: HWND) {
    let target: IDropTarget = Target.into();
    unsafe {
        // The legacy file-drop path is also enabled for Explorer compatibility.
        DragAcceptFiles(hwnd, true);
        if let Err(err) = RegisterDragDrop(hwnd, &target) {
            let log = format!("Shelf OLE RegisterDragDrop failed: {err}\nLegacy WM_DROPFILES and clipboard fallback enabled.\n");
            let _ = std::fs::write(crate::store::data_dir().join("shelf-diagnostics.log"), log);
        }
    }
}

/// Start dragging `path` out of the notch. Blocks until the drop or cancel.
pub fn drag_out(hwnd: HWND, path: &str) {
    unsafe {
        let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(path), None::<&IBindCtx>) else { return };
        let Ok(data) = item.BindToHandler::<_, IDataObject>(None::<&IBindCtx>, &BHID_DataObject) else { return };
        let _ = SHDoDragDrop(Some(hwnd), &data, None::<&IDropSource>, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK);
    }
}
