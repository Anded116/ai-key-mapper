//! Real JoyShockMapper presets from the user's GyroConfigs, converted and checked by the engine.

use std::path::PathBuf;

use engine::profile::{Binding, MoveMode};
use engine::{Btn, Output, Profile};
use jsm_import::{import, Imported, Options};

fn fixture(name: &str) -> Imported {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/GyroConfigs").join(name);
    import(&path, &Options::default()).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn keys(names: &[&str]) -> Vec<Output> {
    names.iter().map(|n| Output::parse(n).unwrap()).collect()
}

fn press(p: &Profile, b: Btn) -> Vec<Output> {
    p.buttons[&b].press.clone()
}

fn layer(p: &Profile, button: Btn, b: Btn) -> &Binding {
    &p.layers.iter().find(|l| l.button == button).unwrap().bindings[&b]
}

fn skipped(im: &Imported, needle: &str) -> bool {
    im.skipped.iter().any(|s| s.contains(needle))
}

#[test]
fn dusk_weapon_state_files_become_pairs() {
    let im = fixture("Dusk - Copy2.txt");
    let p = &im.profile;
    assert_eq!((p.calibration.real_world, p.calibration.in_game_sens), (2.8649, 0.1));
    assert_eq!(p.movement.mode, MoveMode::Wasd);
    assert_eq!(p.movement.keys.to_vec(), keys(&["E", "S", "D", "F"]));
    assert_eq!(p.gyro.off, Some(Btn::Down));
    assert!(p.flick.enabled);
    assert_eq!((p.gyro.min_sens, p.gyro.max_sens), (2.3, 3.5));
    assert_eq!(press(p, Btn::Left), keys(&["WheelUp"]));
    assert_eq!(press(p, Btn::Triangle), keys(&["LShift"]));
    // □ 1/2, ✕ 3/4, ○ 8/9, △ 6/7 from Dusk/{Neutral,W,S,E,N}.txt
    for (b, first, second) in [(Btn::Square, "1", "2"), (Btn::Cross, "3", "4"), (Btn::Circle, "8", "9"), (Btn::Triangle, "6", "7")] {
        let w = layer(p, Btn::R2, b);
        assert_eq!((w.press.clone(), w.next.clone()), (keys(&[first]), keys(&[second])), "{b:?}");
    }
    assert!(im.toml.contains("[weapons]\nslots = 8"));
    assert!(skipped(&im, "[_3Dmouse.txt] STICK_SENS = 360"));
}

#[test]
fn dying_light_tap_hold_double_and_touchpad_halves() {
    let im = fixture("DyingLight_TheBeast.txt");
    let p = &im.profile;
    assert_eq!((p.calibration.real_world, p.calibration.in_game_sens), (400.0, 50.0));
    assert_eq!(p.movement.mode, MoveMode::Hybrid);
    assert_eq!(p.buttons[&Btn::Options].tap, keys(&["I"]));
    assert_eq!(p.buttons[&Btn::Options].hold, keys(&["Esc"]));
    assert_eq!(p.buttons[&Btn::Create].tap, keys(&["N"]));
    assert_eq!(p.buttons[&Btn::Square].press, keys(&["F"]));
    assert_eq!(p.buttons[&Btn::Square].double, keys(&["Z"]));
    // R2 keeps its own key and is the chord layer.
    assert_eq!(press(p, Btn::R2), keys(&["LShift"]));
    assert_eq!(layer(p, Btn::R2, Btn::Cross).press, keys(&["T"]));
    assert_eq!(press(p, Btn::PadLeft), keys(&["R"]));
    assert_eq!(press(p, Btn::PadRight), keys(&["H"]));
    assert!(!p.buttons.contains_key(&Btn::Touchpad));
    assert_eq!(p.gyro.off, Some(Btn::Touch));
    // ZR,S = T is not a weapon: the layer stays explicit.
    assert!(!im.toml.contains("[weapons]"));
    assert!(skipped(&im, "L3+R3 = Z+X"));
    assert!(skipped(&im, "LUP = V"));
}

#[test]
fn ultrakill_four_slots_and_chords() {
    let im = fixture("ULTRAKILL_1.txt");
    let p = &im.profile;
    assert_eq!(p.gyro.off, Some(Btn::L3));
    assert_eq!(p.movement.keys.to_vec(), keys(&["W", "A", "S", "D"]));
    // Later lines win: `- = F5` then `- = TAB`.
    assert_eq!(press(p, Btn::Create), keys(&["Tab"]));
    assert_eq!(press(p, Btn::Touchpad), keys(&["G"]));
    assert!(im.toml.contains("[weapons]\nslots = 4\nkeys = [\"1\", \"3\", \"2\", \"4\"]"));
    assert_eq!(layer(p, Btn::R2, Btn::Circle).press, keys(&["2"]));
    assert!(layer(p, Btn::R2, Btn::Circle).next.is_empty());
    assert_eq!(layer(p, Btn::R2, Btn::R1).press, keys(&["MouseRight"]));
    assert_eq!(layer(p, Btn::R2, Btn::Options).press, keys(&["F9"]));
    assert!(im.toml.contains("# Melee"), "preset comments are kept");
}

#[test]
fn elden_ring_unsupported_touch_grid_and_gyro_on() {
    let im = fixture("EldenRing_FlickStick_GyroOnTouch.txt");
    let p = &im.profile;
    assert_eq!(p.gyro.off, None);
    assert!(!p.buttons.contains_key(&Btn::PadLeft));
    for line in ["GRID_SIZE = 2 2", "T3,CAPTURE = 3", "GYRO_ON = TOUCH"] {
        assert!(skipped(&im, line), "{line}");
    }
    assert!(im.toml.contains("# Не перенесено:"));
}

#[test]
fn virtual_controller_lines_are_listed() {
    let im = fixture("xbox gyro_offontouch.txt");
    assert!(skipped(&im, "VIRTUAL_CONTROLLER = XBOX"));
    assert!(skipped(&im, "X_LB: кнопка виртуального геймпада"));
    assert_eq!(im.profile.gyro.off, Some(Btn::Touch));
    assert_eq!((im.profile.gyro.min_sens, im.profile.gyro.max_sens), (1.3, 2.5));
}

#[test]
fn testrange_reference_preset_matches_the_engine_profile() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../jsm/AutoLoad/testrange.txt");
    let im = import(&path, &Options::default()).unwrap();
    let p = &im.profile;
    let reference = Profile::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../profiles/testrange-hybrid.toml")).unwrap();
    assert_eq!(p.movement.mode, MoveMode::Hybrid);
    assert_eq!(p.movement.walk, reference.movement.walk);
    assert_eq!(p.movement.keys, reference.movement.keys);
    assert_eq!(layer(p, Btn::R2, Btn::L3).press, keys(&["ToggleMovement"]));
    for b in [Btn::Square, Btn::Cross, Btn::Circle, Btn::Triangle, Btn::Create, Btn::Options] {
        assert_eq!(layer(p, Btn::R2, b), layer(&reference, Btn::R2, b), "R2 + {b:?}");
    }
    for b in [Btn::L1, Btn::R1, Btn::L2, Btn::L3, Btn::R3, Btn::Cross, Btn::Square, Btn::Up, Btn::Down, Btn::PadLeft, Btn::Mic] {
        assert_eq!(p.buttons.get(&b), reference.buttons.get(&b), "{b:?}");
    }
}
