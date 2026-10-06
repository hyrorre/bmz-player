//! winit 0.30's public Linux scancode API consumes EV_KEY codes (not XKB's
//! offset-by-eight codes). Reuse its table and our existing physical naming.
use bmz_gameplay::input::backend::PhysicalControl;
use winit::{keyboard::PhysicalKey, platform::scancode::PhysicalKeyExtScancode};

pub(super) fn control(code: u16) -> Option<PhysicalControl> {
    if code >= 0x100 {
        return None;
    } // BTN_* is not keyboard EV_KEY input.
    super::winit::physical_key_to_control(PhysicalKey::from_scancode(u32::from(code)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linux_scancodes_match_existing_ansi_jis_and_modifier_bindings() {
        for (code, name) in [
            (30, "A"),
            (44, "Z"),
            (42, "LShift"),
            (54, "RShift"),
            (29, "LControl"),
            (97, "RControl"),
            (56, "LAlt"),
            (100, "RAlt"),
            (89, "IntlRo"),
            (124, "IntlYen"),
            (92, "Convert"),
            (94, "NonConvert"),
            (79, "Numpad1"),
            (96, "NumpadEnter"),
            (59, "F1"),
            (88, "F12"),
        ] {
            assert_eq!(control(code), Some(PhysicalControl::KeyboardKey(name.into())), "{code}");
        }
        assert_eq!(control(0x110), None);
    }
}
