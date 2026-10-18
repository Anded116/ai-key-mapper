//! Правка привязок из окна. Пишет в файл профиля игры, сохраняя порядок строк, отступы и
//! комментарии; если профиль после правки не загружается, файл возвращается как был.

use std::path::Path;

use engine::Profile;
use serde::Deserialize;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};

/// Привязка из редактора: имена клавиш движка по полям профиля.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Edit {
    pub press: Vec<String>,
    pub tap: Vec<String>,
    pub hold: Vec<String>,
    pub double: Vec<String>,
    pub next: Vec<String>,
    pub latch: Vec<String>,
    pub hold_ms: Option<u32>,
}

/// Столбец, с которого в профилях начинаются комментарии.
const COMMENT_COLUMN: usize = 32;

fn keys(list: &[String]) -> Value {
    match list {
        [one] => one.as_str().into(),
        many => Value::Array(many.iter().map(String::as_str).collect::<Array>()),
    }
}

fn value(e: &Edit) -> Value {
    let only_press = e.tap.is_empty() && e.hold.is_empty() && e.double.is_empty() && e.next.is_empty() && e.latch.is_empty();
    if only_press {
        // Пустой `press` — "": кнопка ничего не делает, даже если в базовом профиле что-то было.
        return if e.press.is_empty() { "".into() } else { keys(&e.press) };
    }
    let mut t = InlineTable::new();
    for (field, list) in [("press", &e.press), ("tap", &e.tap), ("hold", &e.hold), ("double", &e.double), ("next", &e.next), ("latch", &e.latch)] {
        if !list.is_empty() {
            t.insert(field, keys(list));
        }
    }
    if let Some(ms) = e.hold_ms {
        t.insert("hold_ms", i64::from(ms).into());
    }
    Value::InlineTable(t)
}

/// `[buttons]` или `[layer.<кнопка>]`; создаётся, если его нет.
fn section<'a>(doc: &'a mut DocumentMut, layer: Option<&str>) -> &'a mut Table {
    let Some(layer) = layer else {
        if !doc.contains_table("buttons") {
            doc.insert("buttons", toml_edit::table());
        }
        return doc["buttons"].as_table_mut().expect("buttons is a table");
    };
    if !doc.contains_key("layer") {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert("layer", Item::Table(t));
    }
    let layers = doc["layer"].as_table_mut().expect("layer is a table");
    if !layers.contains_key(layer) {
        layers.insert(layer, toml_edit::table());
    }
    layers[layer].as_table_mut().expect("layer.* is a table")
}

/// Применить правку к тексту профиля, проверить, что он грузится, и записать.
fn write(path: &Path, change: impl FnOnce(&mut DocumentMut)) -> Result<(), String> {
    let old = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut doc: DocumentMut = old.parse().map_err(|e| format!("{}: {e}", path.display()))?;
    change(&mut doc);
    std::fs::write(path, doc.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Err(e) = Profile::load(path) {
        let _ = std::fs::write(path, old);
        return Err(format!("профиль не принял правку: {e}"));
    }
    Ok(())
}

pub fn set(path: &Path, button: &str, layer: Option<&str>, edit: &Edit, note: &str) -> Result<(), String> {
    let mut v = value(edit);
    let line = button.len() + 3 + v.to_string().len();
    let gap = if line < COMMENT_COLUMN { COMMENT_COLUMN - line } else { 3 };
    v.decor_mut().set_prefix(" ");
    let note = note.trim();
    v.decor_mut().set_suffix(if note.is_empty() { String::new() } else { format!("{}# {note}", " ".repeat(gap)) });
    write(path, |doc| {
        section(doc, layer).insert(button, Item::Value(v));
    })
}

/// `calibration.real_world`: значение меняется, комментарий у строки остаётся.
pub fn set_real_world(path: &Path, value: f32) -> Result<(), String> {
    let rounded = (f64::from(value) * 10_000.0).round() / 10_000.0;
    set_setting(path, "calibration", "real_world", Value::from(rounded))
}

/// `[controller] hide`: прятать геймпад от игры через HidHide.
pub fn set_hide(path: &Path, hide: bool) -> Result<(), String> {
    set_setting(path, "controller", "hide", Value::from(hide))
}

/// Настройка в разделе профиля; комментарий у прежней строки остаётся.
fn set_setting(path: &Path, section: &str, key: &str, mut value: Value) -> Result<(), String> {
    write(path, |doc| {
        if !doc.contains_table(section) {
            doc.insert(section, toml_edit::table());
        }
        let table = doc[section].as_table_mut().expect("section is a table");
        if let Some(old) = table.get(key).and_then(|i| i.as_value()) {
            *value.decor_mut() = old.decor().clone();
        }
        table.insert(key, Item::Value(value));
    })
}

/// Язык подсказок игры: `prompts = "ru"` или `"en"` в корне профиля.
pub fn set_prompts(path: &Path, lang: &str) -> Result<(), String> {
    write(path, |doc| {
        doc.insert("prompts", toml_edit::value(lang));
    })
}

/// Убрать привязку из файла игры: снова действует то, что в базовом профиле.
pub fn reset(path: &Path, button: &str, layer: Option<&str>) -> Result<(), String> {
    write(path, |doc| {
        section(doc, layer).remove(button);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("aikm-edit-{}-{}", std::process::id(), text.len()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("game.toml");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn edits_keep_layout_and_comments() {
        let path = temp("game = \"G\"\n\n[buttons]\nl1 = \"Space\"                    # прыжок\nr1 = \"MouseLeft\"                # огонь\n");
        let tap_hold = Edit { tap: vec!["Esc".into()], hold: vec!["M".into()], hold_ms: Some(500), ..Edit::default() };
        set(&path, "options", None, &tap_hold, "пауза / карта").unwrap();
        set(&path, "l1", None, &Edit { press: vec!["C".into()], ..Edit::default() }, "присесть").unwrap();
        set(&path, "dpad_down", Some("r2"), &Edit { press: vec!["Z".into()], ..Edit::default() }, "сон").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("l1 = \"C\"                        # присесть\n"), "{text}");
        assert!(text.contains("r1 = \"MouseLeft\"                # огонь\n"), "{text}");
        assert!(text.contains("options = { tap = \"Esc\", hold = \"M\", hold_ms = 500 }   # пауза / карта"), "{text}");
        assert!(text.contains("[layer.r2]\ndpad_down = \"Z\""), "{text}");
        std::fs::write(&path, std::fs::read_to_string(&path).unwrap() + "\n[calibration]\nreal_world = 40.0               # подобрано фликом\n").unwrap();
        set_real_world(&path, 12.5).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("real_world = 12.5               # подобрано фликом"));
        set_prompts(&path, "en").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.find("prompts = \"en\"").unwrap() < text.find("[buttons]").unwrap(), "{text}");
        reset(&path, "options", None).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("options"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn broken_edit_is_rolled_back() {
        let path = temp("game = \"G\"\n[buttons]\nl1 = \"Space\"\n");
        let bad = Edit { press: vec!["NoSuchKey".into()], ..Edit::default() };
        assert!(set(&path, "l1", None, &bad, "").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "game = \"G\"\n[buttons]\nl1 = \"Space\"\n");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
