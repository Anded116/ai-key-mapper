//! Steam Input check. Steam opens the controller when it starts, long before any game, so hiding
//! the pad does not reach it: with Steam Input on, Steam hands the pad to the game as an Xbox
//! controller and the game sees a gamepad next to our keyboard and mouse.

use std::path::{Path, PathBuf};

/// Steam app id of a game installed under `<library>\steamapps\common\<installdir>\`.
pub fn app_id(exe_path: &Path) -> Option<String> {
    let parts: Vec<_> = exe_path.components().collect();
    let common = parts.windows(2).position(|w| {
        w[0].as_os_str().eq_ignore_ascii_case("steamapps") && w[1].as_os_str().eq_ignore_ascii_case("common")
    })?;
    let install_dir = parts.get(common + 2)?.as_os_str().to_string_lossy().to_string();
    let steamapps: PathBuf = parts[..=common].iter().collect();
    for entry in std::fs::read_dir(&steamapps).ok()?.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name.strip_prefix("appmanifest_").and_then(|n| n.strip_suffix(".acf")) else { continue };
        let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
        if vdf_value(&text, "installdir").is_some_and(|d| d.eq_ignore_ascii_case(&install_dir)) {
            return Some(id.to_string());
        }
    }
    None
}

/// Whether Steam Input may take over the game: unless it is switched off for the game itself
/// (Properties → Controller) or for PlayStation controllers altogether.
pub fn input_enabled(app_id: &str) -> Option<bool> {
    let config = std::fs::read_to_string(local_config()?).ok()?;
    if per_app_setting(&config, app_id).as_deref() == Some("0") {
        return Some(false);
    }
    Some(vdf_value(&config, "SteamController_PSSupport").as_deref() != Some("0"))
}

/// What to tell the user when a game whose pad must stay hidden starts through Steam Input.
pub fn input_warning(exe_path: &Path) -> Option<String> {
    let id = app_id(exe_path)?;
    input_enabled(&id)?.then(|| {
        format!(
            "Steam Input для этой игры (app {id}) не выключен: Steam отдаёт ей геймпад в обход скрытия. \
             Выключи: Steam → игра → Свойства → Контроллер → «Отключить Steam Input», и перезапусти игру."
        )
    })
}

fn steam_root() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let mut buf = [0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: the buffer and its byte length describe the same allocation.
    let res = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Valve\\Steam"),
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if res.is_err() {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..chars])))
}

/// `localconfig.vdf` of the account that used Steam last.
fn local_config() -> Option<PathBuf> {
    std::fs::read_dir(steam_root()?.join("userdata"))
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("config").join("localconfig.vdf"))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// First `"key"  "value"` pair in a VDF text.
fn vdf_value(text: &str, key: &str) -> Option<String> {
    let quoted = format!("\"{key}\"");
    let rest = &text[text.find(&quoted)? + quoted.len()..];
    let start = rest.find('"')? + 1;
    let end = start + rest[start..].find('"')?;
    rest[..start - 1].trim().is_empty().then(|| rest[start..end].to_string())
}

/// `UseSteamControllerConfig` of an app: "0" off, "1" default, "2" on. The app's controller block is
/// the `"<id>" { … }` without nested blocks; other blocks with the same id (play time, cloud) nest.
fn per_app_setting(config: &str, app_id: &str) -> Option<String> {
    let quoted = format!("\"{app_id}\"");
    let mut from = 0;
    while let Some(at) = config[from..].find(&quoted) {
        let after = &config[from + at + quoted.len()..];
        from += at + quoted.len();
        let Some(body) = after.trim_start().strip_prefix('{') else { continue };
        let Some(end) = body.find('}') else { continue };
        let block = &body[..end];
        if !block.contains('{') {
            if let Some(v) = vdf_value(block, "UseSteamControllerConfig") {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
"UserLocalConfigStore"
{
	"Software" { "Valve" { "Steam" { "apps" {
		"1326470"
		{
			"LastPlayed"		"1792344976"
			"cloud" { "last_sync_state" "synchronized" }
		}
	} } } }
	"apps"
	{
		"208650"
		{
			"UseSteamControllerConfig"		"0"
			"SteamControllerRumble"		"-1"
		}
		"553850"
		{
			"UseSteamControllerConfig"		"2"
		}
	}
	"SteamController_PSSupport"		"2"
}"#;

    #[test]
    fn reads_per_app_steam_input() {
        assert_eq!(per_app_setting(CONFIG, "208650").as_deref(), Some("0"));
        assert_eq!(per_app_setting(CONFIG, "553850").as_deref(), Some("2"));
        assert_eq!(per_app_setting(CONFIG, "1326470"), None);
        assert_eq!(vdf_value(CONFIG, "SteamController_PSSupport").as_deref(), Some("2"));
    }

    #[test]
    fn finds_app_id_by_install_dir() {
        let lib = std::env::temp_dir().join(format!("aikm-steam-{}", std::process::id()));
        let game = lib.join("steamapps").join("common").join("Sons Of The Forest");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(
            lib.join("steamapps").join("appmanifest_1326470.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1326470\"\n\t\"installdir\"\t\t\"Sons Of The Forest\"\n}\n",
        )
        .unwrap();
        let id = app_id(&game.join("SonsOfTheForest.exe"));
        let _ = std::fs::remove_dir_all(&lib);
        assert_eq!(id.as_deref(), Some("1326470"));
        assert_eq!(app_id(Path::new("C:/Games/Other/game.exe")), None);
    }
}
