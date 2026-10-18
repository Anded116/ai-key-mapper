//! IOCTL codes of the HidHide control device and the MULTI_SZ format of its lists.

/// `CTL_CODE(32769, n, METHOD_BUFFERED, FILE_READ_DATA)`.
const fn ctl(n: u32) -> u32 {
    0x8001_0000 | 0x4000 | (n << 2)
}

pub const GET_WHITELIST: u32 = ctl(2048);
pub const SET_WHITELIST: u32 = ctl(2049);
pub const GET_BLACKLIST: u32 = ctl(2050);
pub const SET_BLACKLIST: u32 = ctl(2051);
pub const GET_ACTIVE: u32 = ctl(2052);
pub const SET_ACTIVE: u32 = ctl(2053);
pub const GET_WLINVERSE: u32 = ctl(2054);
pub const SET_WLINVERSE: u32 = ctl(2055);
/// Driver 1.7+ only.
pub const ADD_SESSION_BLACKLIST: u32 = ctl(2056);
/// Driver 1.7+ only.
pub const CLR_SESSION_BLACKLIST: u32 = ctl(2057);

/// `"a\0b\0\0"`; an empty list is `"\0\0"`. Empty items are dropped: inside MULTI_SZ
/// they would read as the end of the list.
pub fn encode_multi_sz(items: &[String]) -> Vec<u16> {
    let mut out = Vec::new();
    for item in items.iter().filter(|s| !s.is_empty()) {
        out.extend(item.encode_utf16());
        out.push(0);
    }
    if out.is_empty() {
        out.push(0);
    }
    out.push(0);
    out
}

/// Stops at the first empty string; tolerates a missing final terminator.
pub fn decode_multi_sz(buf: &[u16]) -> Vec<String> {
    buf.split(|&c| c == 0)
        .take_while(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn codes_match_driver_contract() {
        assert_eq!(GET_WHITELIST, 0x8001_6000);
        assert_eq!(SET_BLACKLIST, 0x8001_600C);
        assert_eq!(SET_WLINVERSE, 0x8001_601C);
        assert_eq!(CLR_SESSION_BLACKLIST, 0x8001_6024);
    }

    #[test]
    fn encode() {
        assert_eq!(encode_multi_sz(&[]), w("\0\0"));
        assert_eq!(encode_multi_sz(&["a".into(), "bc".into()]), w("a\0bc\0\0"));
        assert_eq!(encode_multi_sz(&["".into(), "a".into()]), w("a\0\0"));
    }

    #[test]
    fn decode() {
        assert!(decode_multi_sz(&w("\0\0")).is_empty());
        assert!(decode_multi_sz(&[]).is_empty());
        assert_eq!(decode_multi_sz(&w("a\0bc\0\0")), ["a", "bc"]);
        assert_eq!(decode_multi_sz(&w("a\0bc")), ["a", "bc"]);
        assert_eq!(decode_multi_sz(&w("a\0\0junk\0")), ["a"]);
    }

    #[test]
    fn round_trip() {
        let items = vec![
            r"HID\VID_054C&PID_0CE6&MI_03\7&30dcb22d&0&0000".to_string(),
            r"\Device\HarddiskVolume3\Игры\game.exe".to_string(),
        ];
        assert_eq!(decode_multi_sz(&encode_multi_sz(&items)), items);
    }
}
