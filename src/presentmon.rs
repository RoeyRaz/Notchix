//! Optional read-only connection to Intel PresentMon's installed API2 service.
//! No game injection or process-memory access. If the service is absent or a
//! metric is unavailable, callers receive `None` rather than an estimate.

use std::ffi::{c_void, CStr};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows::core::{PCSTR, PCWSTR};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

#[link(name = "kernel32")]
extern "system" {
    fn FreeLibrary(module: HMODULE) -> i32;
}

type Handle = *mut c_void;
type Open = unsafe extern "C" fn(*mut Handle) -> i32;
type Close = unsafe extern "C" fn(Handle) -> i32;
type Track = unsafe extern "C" fn(Handle, u32) -> i32;
type Register = unsafe extern "C" fn(Handle, *mut Handle, *mut QueryElement, u64, f64, f64) -> i32;
type FreeQuery = unsafe extern "C" fn(Handle) -> i32;
type Poll = unsafe extern "C" fn(Handle, u32, *mut u8, *mut u32) -> i32;
type GetRoot = unsafe extern "C" fn(Handle, *mut *const Root) -> i32;
type FreeRoot = unsafe extern "C" fn(*const Root) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
struct QueryElement {
    metric: u32,
    stat: u32,
    device_id: u32,
    array_index: u32,
    data_offset: u64,
    data_size: u64,
}

impl QueryElement {
    fn average(metric: u32, device_id: u32) -> Self {
        Self {
            metric,
            stat: 1,
            device_id,
            array_index: 0,
            data_offset: 0,
            data_size: 0,
        }
    }
}

#[repr(C)]
struct ObjectArray {
    data: *const *const c_void,
    size: usize,
}
#[repr(C)]
struct Root {
    metrics: *const ObjectArray,
    enums: *const ObjectArray,
    devices: *const ObjectArray,
    units: *const ObjectArray,
}
#[repr(C)]
struct Device {
    id: u32,
    kind: u32,
    vendor: u32,
    name: *const c_void,
    luid: *const c_void,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Metrics {
    pub fps: Option<f64>,
    pub gpu_percent: Option<f64>,
}

pub struct PresentMon {
    dll: HMODULE,
    session: Handle,
    query: Handle,
    pid: u32,
    elements: Vec<QueryElement>,
    close: Close,
    stop: Track,
    free_query: FreeQuery,
    poll: Poll,
}

impl PresentMon {
    pub fn connect(pid: u32) -> Option<Self> {
        if pid == 0 {
            return None;
        }
        // Load only the installed, absolute Intel path. Avoid DLL search-order
        // hijacking and never bundle a service binary with the app.
        let base = PathBuf::from(std::env::var_os("ProgramFiles")?).join("Intel\\PresentMon");
        let path = [
            base.join("PresentMonAPI2.dll"),
            base.join("SDK\\PresentMonAPI2.dll"),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file())?;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let dll = unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())).ok()? };
        let opened = unsafe { Self::open(dll, pid) };
        if opened.is_none() {
            unsafe {
                let _ = FreeLibrary(dll);
            }
        }
        opened
    }

    unsafe fn open(dll: HMODULE, pid: u32) -> Option<Self> {
        unsafe fn entry<T: Copy>(dll: HMODULE, name: &'static CStr) -> Option<T> {
            let raw = GetProcAddress(dll, PCSTR(name.as_ptr() as *const u8))?;
            Some(std::mem::transmute_copy(&raw))
        }
        let open: Open = entry(dll, c"pmOpenSession")?;
        let close: Close = entry(dll, c"pmCloseSession")?;
        let start: Track = entry(dll, c"pmStartTrackingProcess")?;
        let stop: Track = entry(dll, c"pmStopTrackingProcess")?;
        let register: Register = entry(dll, c"pmRegisterDynamicQuery")?;
        let free_query: FreeQuery = entry(dll, c"pmFreeDynamicQuery")?;
        let poll: Poll = entry(dll, c"pmPollDynamicQuery")?;
        let get_root: GetRoot = entry(dll, c"pmGetIntrospectionRoot")?;
        let free_root: FreeRoot = entry(dll, c"pmFreeIntrospectionRoot")?;
        let mut session = std::ptr::null_mut();
        if open(&mut session) != 0 || session.is_null() {
            return None;
        }
        if start(session, pid) != 0 {
            let _ = close(session);
            return None;
        }

        // Displayed FPS is per process. GPU utilization is per graphics
        // adapter; query only a real adapter reported by the service.
        let mut elements = vec![QueryElement::average(11, 0)];
        let mut root = std::ptr::null();
        if get_root(session, &mut root) == 0 && !root.is_null() {
            let devices = (*root).devices;
            if !devices.is_null() && !(*devices).data.is_null() {
                for i in 0..(*devices).size.min(16) {
                    let ptr = *(*devices).data.add(i) as *const Device;
                    if !ptr.is_null() && (*ptr).kind == 1 {
                        elements.push(QueryElement::average(32, (*ptr).id));
                        break;
                    }
                }
            }
            let _ = free_root(root);
        }
        let mut query = std::ptr::null_mut();
        let mut status = register(
            session,
            &mut query,
            elements.as_mut_ptr(),
            elements.len() as u64,
            1000.0,
            0.0,
        );
        if status != 0 && elements.len() == 2 {
            elements.pop();
            status = register(session, &mut query, elements.as_mut_ptr(), 1, 1000.0, 0.0);
        }
        if status != 0 || query.is_null() {
            let _ = stop(session, pid);
            let _ = close(session);
            return None;
        }
        Some(Self {
            dll,
            session,
            query,
            pid,
            elements,
            close,
            stop,
            free_query,
            poll,
        })
    }

    pub fn sample(&mut self) -> Metrics {
        // API2 writes one result per swap chain. Bound the buffer and only read
        // offsets the API registered as eight-byte doubles inside that buffer.
        let row_size = self
            .elements
            .iter()
            .filter_map(|e| e.data_offset.checked_add(e.data_size))
            .max()
            .unwrap_or(0);
        if row_size == 0 || row_size > 4096 {
            return Metrics::default();
        }
        let mut blob = vec![0u8; row_size as usize * 4];
        let mut chains = 4u32;
        if unsafe { (self.poll)(self.query, self.pid, blob.as_mut_ptr(), &mut chains) } != 0
            || chains == 0
        {
            return Metrics::default();
        }
        let read = |e: &QueryElement| -> Option<f64> {
            if e.data_size != 8 {
                return None;
            }
            let end = e.data_offset.checked_add(8)? as usize;
            let bytes: [u8; 8] = blob.get(e.data_offset as usize..end)?.try_into().ok()?;
            let value = f64::from_ne_bytes(bytes);
            value.is_finite().then_some(value)
        };
        let fps = read(&self.elements[0]).filter(|v| (1.0..=1000.0).contains(v));
        let gpu_percent = self
            .elements
            .get(1)
            .and_then(read)
            .filter(|v| (0.0..=100.0).contains(v));
        Metrics { fps, gpu_percent }
    }
}

impl Drop for PresentMon {
    fn drop(&mut self) {
        unsafe {
            let _ = (self.free_query)(self.query);
            let _ = (self.stop)(self.session, self.pid);
            let _ = (self.close)(self.session);
            let _ = FreeLibrary(self.dll);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_service_has_no_metrics() {
        if std::env::var_os("ProgramFiles").is_none() {
            return;
        }
        let base =
            PathBuf::from(std::env::var_os("ProgramFiles").unwrap()).join("Intel\\PresentMon");
        if !base.join("PresentMonAPI2.dll").exists()
            && !base.join("SDK\\PresentMonAPI2.dll").exists()
        {
            assert!(PresentMon::connect(std::process::id()).is_none());
        }
    }
}
