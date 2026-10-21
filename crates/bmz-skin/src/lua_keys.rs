//! Keyboard state for beatoraja's `main_state.key_pressed`.
//!
//! beatoraja passes libGDX `Input.Keys` codes, or their names through
//! `Input.Keys.valueOf`, to `Gdx.input.isKeyPressed`. The table below is the
//! one in the libGDX build bundled with beatoraja (`lib/gdx.jar`), where for
//! example `Escape` is 131 and `F1` is 244.

/// Number of `u64` words in [`LuaKeyboardState`]; keycodes are `0..256`.
pub const LUA_KEYBOARD_WORDS: usize = 4;

/// Bit set of pressed libGDX keycodes.
pub type LuaKeyboardState = [u64; LUA_KEYBOARD_WORDS];

/// `Input.Keys.toString` for every defined keycode of the bundled libGDX.
const GDX_KEY_NAMES: &[(i32, &str)] = &[
    (0, "Unknown"),
    (1, "Soft Left"),
    (2, "Soft Right"),
    (3, "Home"),
    (4, "Back"),
    (5, "Call"),
    (6, "End Call"),
    (7, "0"),
    (8, "1"),
    (9, "2"),
    (10, "3"),
    (11, "4"),
    (12, "5"),
    (13, "6"),
    (14, "7"),
    (15, "8"),
    (16, "9"),
    (17, "*"),
    (18, "#"),
    (19, "Up"),
    (20, "Down"),
    (21, "Left"),
    (22, "Right"),
    (23, "Center"),
    (24, "Volume Up"),
    (25, "Volume Down"),
    (26, "Power"),
    (27, "Camera"),
    (28, "Clear"),
    (29, "A"),
    (30, "B"),
    (31, "C"),
    (32, "D"),
    (33, "E"),
    (34, "F"),
    (35, "G"),
    (36, "H"),
    (37, "I"),
    (38, "J"),
    (39, "K"),
    (40, "L"),
    (41, "M"),
    (42, "N"),
    (43, "O"),
    (44, "P"),
    (45, "Q"),
    (46, "R"),
    (47, "S"),
    (48, "T"),
    (49, "U"),
    (50, "V"),
    (51, "W"),
    (52, "X"),
    (53, "Y"),
    (54, "Z"),
    (55, ","),
    (56, "."),
    (57, "L-Alt"),
    (58, "R-Alt"),
    (59, "L-Shift"),
    (60, "R-Shift"),
    (61, "Tab"),
    (62, "Space"),
    (63, "SYM"),
    (64, "Explorer"),
    (65, "Envelope"),
    (66, "Enter"),
    (67, "Delete"),
    (68, "`"),
    (69, "-"),
    (70, "="),
    (71, "["),
    (72, "]"),
    (73, "\\"),
    (74, ";"),
    (75, "'"),
    (76, "/"),
    (77, "@"),
    (78, "Num"),
    (79, "Headset Hook"),
    (80, "Focus"),
    (81, "Plus"),
    (82, "Menu"),
    (83, "Notification"),
    (84, "Search"),
    (85, "Play/Pause"),
    (86, "Stop Media"),
    (87, "Next Media"),
    (88, "Prev Media"),
    (89, "Rewind"),
    (90, "Fast Forward"),
    (91, "Mute"),
    (92, "Page Up"),
    (93, "Page Down"),
    (94, "PICTSYMBOLS"),
    (95, "SWITCH_CHARSET"),
    (96, "A Button"),
    (97, "B Button"),
    (98, "C Button"),
    (99, "X Button"),
    (100, "Y Button"),
    (101, "Z Button"),
    (102, "L1 Button"),
    (103, "R1 Button"),
    (104, "L2 Button"),
    (105, "R2 Button"),
    (106, "Left Thumb"),
    (107, "Right Thumb"),
    (108, "Start"),
    (109, "Select"),
    (110, "Button Mode"),
    (112, "Forward Delete"),
    (129, "L-Ctrl"),
    (130, "R-Ctrl"),
    (131, "Escape"),
    (132, "End"),
    (133, "Insert"),
    (144, "Numpad 0"),
    (145, "Numpad 1"),
    (146, "Numpad 2"),
    (147, "Numpad 3"),
    (148, "Numpad 4"),
    (149, "Numpad 5"),
    (150, "Numpad 6"),
    (151, "Numpad 7"),
    (152, "Numpad 8"),
    (153, "Numpad 9"),
    (243, ":"),
    (244, "F1"),
    (245, "F2"),
    (246, "F3"),
    (247, "F4"),
    (248, "F5"),
    (249, "F6"),
    (250, "F7"),
    (251, "F8"),
    (252, "F9"),
    (253, "F10"),
    (254, "F11"),
    (255, "F12"),
];

/// BMZ keyboard control names (`input/winit.rs`) and the libGDX keycode the
/// LWJGL desktop backend reports for the same physical key.
const BMZ_KEY_TO_GDX: &[(&str, i32)] = &[
    ("0", 7),
    ("1", 8),
    ("2", 9),
    ("3", 10),
    ("4", 11),
    ("5", 12),
    ("6", 13),
    ("7", 14),
    ("8", 15),
    ("9", 16),
    ("A", 29),
    ("B", 30),
    ("C", 31),
    ("D", 32),
    ("E", 33),
    ("F", 34),
    ("G", 35),
    ("H", 36),
    ("I", 37),
    ("J", 38),
    ("K", 39),
    ("L", 40),
    ("M", 41),
    ("N", 42),
    ("O", 43),
    ("P", 44),
    ("Q", 45),
    ("R", 46),
    ("S", 47),
    ("T", 48),
    ("U", 49),
    ("V", 50),
    ("W", 51),
    ("X", 52),
    ("Y", 53),
    ("Z", 54),
    ("Home", 3),
    ("ArrowUp", 19),
    ("ArrowDown", 20),
    ("ArrowLeft", 21),
    ("ArrowRight", 22),
    ("Comma", 55),
    ("Period", 56),
    ("LAlt", 57),
    ("RAlt", 58),
    ("LShift", 59),
    ("RShift", 60),
    ("Tab", 61),
    ("Space", 62),
    ("Enter", 66),
    ("NumpadEnter", 66),
    ("Backspace", 67),
    ("Backquote", 68),
    ("Minus", 69),
    ("NumpadSubtract", 69),
    ("Equal", 70),
    ("BracketLeft", 71),
    ("BracketRight", 72),
    ("Backslash", 73),
    ("Semicolon", 74),
    ("Quote", 75),
    ("Slash", 76),
    ("NumpadDivide", 76),
    ("NumpadMultiply", 17),
    ("NumpadAdd", 81),
    ("NumpadDecimal", 56),
    ("ContextMenu", 82),
    ("PageUp", 92),
    ("PageDown", 93),
    ("Delete", 112),
    ("LControl", 129),
    ("RControl", 130),
    ("Escape", 131),
    ("End", 132),
    ("Insert", 133),
    ("Numpad0", 144),
    ("Numpad1", 145),
    ("Numpad2", 146),
    ("Numpad3", 147),
    ("Numpad4", 148),
    ("Numpad5", 149),
    ("Numpad6", 150),
    ("Numpad7", 151),
    ("Numpad8", 152),
    ("Numpad9", 153),
    ("F1", 244),
    ("F2", 245),
    ("F3", 246),
    ("F4", 247),
    ("F5", 248),
    ("F6", 249),
    ("F7", 250),
    ("F8", 251),
    ("F9", 252),
    ("F10", 253),
    ("F11", 254),
    ("F12", 255),
];

/// `Input.Keys.valueOf`: exact, case-sensitive names only.
pub fn gdx_keycode_from_name(name: &str) -> Option<i32> {
    GDX_KEY_NAMES.iter().find(|(_, key)| *key == name).map(|(code, _)| *code)
}

/// The libGDX keycode for a BMZ keyboard control name, if the key exists there.
pub fn gdx_keycode_for_bmz_key(name: &str) -> Option<i32> {
    BMZ_KEY_TO_GDX.iter().find(|(key, _)| *key == name).map(|(_, code)| *code)
}

/// Collects held BMZ keyboard controls. Names without a libGDX key, such as
/// controller buttons, are ignored.
pub fn lua_keyboard_state<'a>(names: impl IntoIterator<Item = &'a str>) -> LuaKeyboardState {
    let mut state = [0; LUA_KEYBOARD_WORDS];
    for code in names.into_iter().filter_map(gdx_keycode_for_bmz_key) {
        let code = code as usize;
        state[code / 64] |= 1 << (code % 64);
    }
    state
}

/// `Gdx.input.isKeyPressed(keycode)` for codes `>= 0`; anything else is false.
pub fn lua_keyboard_pressed(state: &LuaKeyboardState, keycode: i32) -> bool {
    usize::try_from(keycode)
        .ok()
        .filter(|code| *code < LUA_KEYBOARD_WORDS * 64)
        .is_some_and(|code| state[code / 64] & (1 << (code % 64)) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdx_names_match_the_bundled_libgdx_table() {
        assert_eq!(GDX_KEY_NAMES.len(), 140);
        for (code, name) in GDX_KEY_NAMES {
            assert_eq!(gdx_keycode_from_name(name), Some(*code), "{name}");
        }
        assert_eq!(gdx_keycode_from_name("Escape"), Some(131));
        assert_eq!(gdx_keycode_from_name("F1"), Some(244));
        assert_eq!(gdx_keycode_from_name("L-Shift"), Some(59));
        assert_eq!(gdx_keycode_from_name("Numpad 0"), Some(144));
        // valueOf is case-sensitive and does not know BMZ names.
        assert_eq!(gdx_keycode_from_name("escape"), None);
        assert_eq!(gdx_keycode_from_name("LShift"), None);
    }

    #[test]
    fn every_bmz_key_maps_to_a_defined_gdx_keycode() {
        for (name, code) in BMZ_KEY_TO_GDX {
            assert!(GDX_KEY_NAMES.iter().any(|(defined, _)| defined == code), "{name} -> {code}");
        }
    }

    #[test]
    fn keyboard_state_holds_mapped_keys_only() {
        let state = lua_keyboard_state(["A", "LShift", "NumpadEnter", "F12", "Button1", "AxisX"]);
        for code in [29, 59, 66, 255] {
            assert!(lua_keyboard_pressed(&state, code), "{code}");
        }
        for code in [-1, 0, 30, 60, 256, i32::MAX, i32::MIN] {
            assert!(!lua_keyboard_pressed(&state, code), "{code}");
        }
        assert_eq!(lua_keyboard_state(["Enter", "NumpadEnter"]), lua_keyboard_state(["Enter"]));
    }
}
