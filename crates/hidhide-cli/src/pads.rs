//! Present DualSense HID nodes. Stand-in until the `devices` crate (ContainerId, hotplug).

use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW,
    DIGCF_PRESENT, GUID_DEVCLASS_HIDCLASS, SP_DEVINFO_DATA,
};

/// DualSense and DualSense Edge. DS4 PIDs are left out on purpose: a ViGEm virtual DS4
/// carries the same VID/PID and must never land in the blacklist.
const PIDS: [&str; 2] = ["0CE6", "0DF2"];

/// Every present HIDClass node of a DualSense: the HID collection and, over Bluetooth,
/// its `BTHENUM\{00001124-…}` parent too, like HidHide Client does.
pub fn dualsense_instances() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(set) = (unsafe { SetupDiGetClassDevsW(Some(&GUID_DEVCLASS_HIDCLASS), PCWSTR::null(), None, DIGCF_PRESENT) })
    else {
        return out;
    };
    for i in 0.. {
        let mut data = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
        if unsafe { SetupDiEnumDeviceInfo(set, i, &mut data) }.is_err() {
            break;
        }
        let mut buf = [0u16; 512];
        if unsafe { SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut buf), None) }.is_ok() {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let id = String::from_utf16_lossy(&buf[..len]);
            if is_dualsense(&id) {
                out.push(id);
            }
        }
    }
    let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };
    out
}

/// USB: `HID\VID_054C&PID_0CE6&MI_03\…`, Bluetooth: `…_VID&0002054C_PID&0CE6\…`.
pub fn is_dualsense(instance: &str) -> bool {
    let id = instance.to_ascii_uppercase();
    let sony = id.contains("VID_054C") || id.contains("VID&0002054C");
    sony && PIDS.iter().any(|p| id.contains(&format!("PID_{p}")) || id.contains(&format!("PID&{p}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_usb_and_bluetooth() {
        assert!(is_dualsense(r"HID\VID_054C&PID_0CE6&MI_03\7&30dcb22d&0&0000"));
        assert!(is_dualsense(r"HID\{00001124-0000-1000-8000-00805F9B34FB}_VID&0002054c_PID&0ce6\8&D004270&3&0000"));
        assert!(is_dualsense(r"BTHENUM\{00001124-0000-1000-8000-00805F9B34FB}_VID&0002054C_PID&0DF2\7&1&0"));
        assert!(!is_dualsense(r"HID\VID_054C&PID_05C4\1&2&3"));
        assert!(!is_dualsense(r"HID\VID_045E&PID_0CE6\1&2&3"));
    }
}
