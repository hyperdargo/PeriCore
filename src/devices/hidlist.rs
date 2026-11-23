//! Lists HID interfaces without asking every device for its name.
//!
//! hidapi's enumeration reads the product strings of *every* HID device, and
//! some built-in devices take seconds to answer (a Synaptics 06CB:1595 control
//! takes 10 s per interface), which stalled PeriCore's whole device scan.
//! Here the vendor id comes from the interface path, so only devices from
//! supported vendors are ever opened, and only for their attributes and usage.

use std::ffi::CString;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_List_SizeW, CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
    CR_BUFFER_SMALL, CR_SUCCESS,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData, HidP_GetCaps, HIDD_ATTRIBUTES,
    HIDP_CAPS, HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};

#[derive(Debug, Clone)]
pub struct HidInfo {
    pub path: CString,
    pub vid: u16,
    pub pid: u16,
    pub usage_page: u16,
    pub usage: u16,
    /// USB interface number (`MI_xx`), or -1 when the path has none.
    pub interface: i32,
}

fn hex_after(s: &str, tag: &str, digits: usize) -> Option<u32> {
    let at = s.find(tag)? + tag.len();
    u32::from_str_radix(s.get(at..at + digits)?, 16).ok()
}

/// Vendor id from an interface path: USB `vid_054c`, Bluetooth `_vid&0002054c`.
pub fn vid_from_path(path: &str) -> Option<u16> {
    let p = path.to_ascii_lowercase();
    hex_after(&p, "_vid&", 8).or_else(|| hex_after(&p, "vid_", 4)).map(|v| (v & 0xFFFF) as u16)
}

/// USB interface number from `mi_xx` in the path.
pub fn interface_from_path(path: &str) -> i32 {
    hex_after(&path.to_ascii_lowercase(), "mi_", 2).map_or(-1, |n| n as i32)
}

/// Every present HID interface path.
fn interface_paths() -> Vec<String> {
    let mut guid = unsafe { std::mem::zeroed() };
    // SAFETY: fills the GUID we pass.
    unsafe { HidD_GetHidGuid(&mut guid) };
    for _ in 0..3 {
        let mut len = 0u32;
        // SAFETY: valid GUID and out pointer; no device id filter.
        if unsafe { CM_Get_Device_Interface_List_SizeW(&mut len, &guid, std::ptr::null(), CM_GET_DEVICE_INTERFACE_LIST_PRESENT) } != CR_SUCCESS {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        // SAFETY: buf holds `len` UTF-16 units as the API asked for.
        let r = unsafe { CM_Get_Device_Interface_ListW(&guid, std::ptr::null(), buf.as_mut_ptr(), len, CM_GET_DEVICE_INTERFACE_LIST_PRESENT) };
        if r == CR_BUFFER_SMALL {
            continue; // a device arrived in between
        }
        if r != CR_SUCCESS {
            return Vec::new();
        }
        return buf.split(|c| *c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect();
    }
    Vec::new()
}

/// Opens one interface without read/write access (works for keyboards and
/// mice too) and reads its ids and top-level usage.
fn query(path: &str) -> Option<HidInfo> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: zero access rights only allow attribute queries; handle closed below.
    let h = unsafe {
        CreateFileW(wide.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut())
    };
    if h == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut attrs: HIDD_ATTRIBUTES = unsafe { std::mem::zeroed() };
    attrs.Size = std::mem::size_of::<HIDD_ATTRIBUTES>() as u32;
    let mut caps: HIDP_CAPS = unsafe { std::mem::zeroed() };
    let mut pre: PHIDP_PREPARSED_DATA = 0;
    // SAFETY: h is a valid HID handle; out structs are sized correctly.
    let ok = unsafe {
        let ok = HidD_GetAttributes(h, &mut attrs) != 0
            && HidD_GetPreparsedData(h, &mut pre) != 0
            && HidP_GetCaps(pre, &mut caps) == HIDP_STATUS_SUCCESS;
        if pre != 0 {
            HidD_FreePreparsedData(pre);
        }
        CloseHandle(h);
        ok
    };
    if !ok {
        return None;
    }
    Some(HidInfo {
        path: CString::new(path).ok()?,
        vid: attrs.VendorID,
        pid: attrs.ProductID,
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        interface: interface_from_path(path),
    })
}

/// HID interfaces whose vendor id is in `vids`.
pub fn list(vids: &[u16]) -> Vec<HidInfo> {
    interface_paths()
        .iter()
        .filter(|p| vid_from_path(p).is_some_and(|v| vids.contains(&v)))
        .filter_map(|p| query(p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_from_paths() {
        let usb = r"\\?\HID#VID_3151&PID_5007&MI_02#8&20cad18c&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        assert_eq!(vid_from_path(usb), Some(0x3151));
        assert_eq!(interface_from_path(usb), 2);
        let bt = r"\\?\HID#{00001124-0000-1000-8000-00805f9b34fb}_VID&0002054c_PID&09cc#a&2377cef4&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        assert_eq!(vid_from_path(bt), Some(0x054C));
        assert_eq!(interface_from_path(bt), -1);
        assert_eq!(vid_from_path(r"\\?\HID#ACPI0011#3&30be55&0&0000#{x}"), None);
    }
}
