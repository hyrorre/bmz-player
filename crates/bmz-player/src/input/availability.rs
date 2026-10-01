//! Supported backend choices, shared by config loading and the settings UI.
use crate::config::app_config::{GamepadBackendKind, GlobalInputConfig, InputBackendKind};

#[derive(Clone, Copy)]
pub(crate) struct BackendAvailability {
    raw_input: bool,
    macos_hid: bool,
    gamecontroller: bool,
    gameinput: bool,
}

impl BackendAvailability {
    pub(crate) fn current() -> Self {
        Self {
            raw_input: cfg!(windows),
            macos_hid: cfg!(target_os = "macos"),
            gamecontroller: {
                #[cfg(target_os = "macos")]
                {
                    super::gamecontroller::is_available()
                }
                #[cfg(not(target_os = "macos"))]
                {
                    false
                }
            },
            gameinput: cfg!(all(windows, feature = "experimental-gameinput")),
        }
    }

    fn supports_keyboard(self, backend: &InputBackendKind) -> bool {
        match backend {
            InputBackendKind::Auto | InputBackendKind::Winit => true,
            InputBackendKind::RawInput => self.raw_input,
            InputBackendKind::MacOsHid => self.macos_hid,
            InputBackendKind::MacOsGameController => self.gamecontroller,
            InputBackendKind::Hid | InputBackendKind::Midi => false,
        }
    }

    fn supports_gamepad(self, backend: GamepadBackendKind) -> bool {
        match backend {
            GamepadBackendKind::Auto | GamepadBackendKind::Gilrs => true,
            GamepadBackendKind::RawInput => self.raw_input,
            GamepadBackendKind::GameController => self.gamecontroller,
            GamepadBackendKind::GameInput => self.gameinput,
        }
    }

    pub(crate) fn keyboards(self) -> impl Iterator<Item = InputBackendKind> {
        [
            InputBackendKind::Auto,
            InputBackendKind::Winit,
            InputBackendKind::RawInput,
            InputBackendKind::MacOsHid,
            InputBackendKind::MacOsGameController,
        ]
        .into_iter()
        .filter(move |backend| self.supports_keyboard(backend))
    }

    pub(crate) fn gamepads(self) -> impl Iterator<Item = GamepadBackendKind> {
        [
            GamepadBackendKind::Auto,
            GamepadBackendKind::Gilrs,
            GamepadBackendKind::RawInput,
            GamepadBackendKind::GameController,
            GamepadBackendKind::GameInput,
        ]
        .into_iter()
        .filter(move |backend| self.supports_gamepad(*backend))
    }

    pub(crate) fn normalize_config(self, config: &mut GlobalInputConfig) {
        if !self.supports_keyboard(&config.backend) {
            tracing::warn!(backend = ?config.backend, "input backend unavailable on this platform; using auto");
            config.backend = InputBackendKind::Auto;
        }
        if !self.supports_gamepad(config.gamepad_backend) {
            tracing::warn!(backend = ?config.gamepad_backend, "gamepad backend unavailable on this platform; using gilrs");
            config.gamepad_backend = GamepadBackendKind::Gilrs;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform(raw_input: bool, macos_hid: bool, gamecontroller: bool) -> BackendAvailability {
        BackendAvailability { raw_input, macos_hid, gamecontroller, gameinput: false }
    }

    #[test]
    fn macos_10_hides_raw_input_and_both_gamecontroller_choices() {
        let available = platform(false, true, false);
        assert_eq!(
            available.keyboards().collect::<Vec<_>>(),
            vec![InputBackendKind::Auto, InputBackendKind::Winit, InputBackendKind::MacOsHid]
        );
        assert_eq!(
            available.gamepads().collect::<Vec<_>>(),
            vec![GamepadBackendKind::Auto, GamepadBackendKind::Gilrs]
        );
        let mut config = crate::config::app_config::AppConfig::default().input;
        config.backend = InputBackendKind::MacOsGameController;
        config.gamepad_backend = GamepadBackendKind::GameController;
        available.normalize_config(&mut config);
        assert_eq!(config.backend, InputBackendKind::Auto);
        assert_eq!(config.gamepad_backend, GamepadBackendKind::Gilrs);
    }

    #[test]
    fn macos_11_keeps_gamecontroller_choices_without_a_connected_device() {
        let available = platform(false, true, true);
        assert_eq!(
            available.keyboards().collect::<Vec<_>>(),
            vec![
                InputBackendKind::Auto,
                InputBackendKind::Winit,
                InputBackendKind::MacOsHid,
                InputBackendKind::MacOsGameController
            ]
        );
        assert_eq!(
            available.gamepads().collect::<Vec<_>>(),
            vec![
                GamepadBackendKind::Auto,
                GamepadBackendKind::Gilrs,
                GamepadBackendKind::GameController
            ]
        );
    }

    #[test]
    fn windows_shows_raw_input_and_gameinput_only_when_enabled() {
        let mut available = platform(true, false, false);
        assert_eq!(
            available.keyboards().collect::<Vec<_>>(),
            vec![InputBackendKind::Auto, InputBackendKind::Winit, InputBackendKind::RawInput]
        );
        assert_eq!(
            available.gamepads().collect::<Vec<_>>(),
            vec![GamepadBackendKind::Auto, GamepadBackendKind::Gilrs, GamepadBackendKind::RawInput]
        );
        available.gameinput = true;
        assert_eq!(available.gamepads().last(), Some(GamepadBackendKind::GameInput));
    }

    #[test]
    fn linux_shows_only_portable_backends_and_preserves_other_input_settings() {
        let available = platform(false, false, false);
        assert_eq!(
            available.keyboards().collect::<Vec<_>>(),
            vec![InputBackendKind::Auto, InputBackendKind::Winit]
        );
        assert_eq!(
            available.gamepads().collect::<Vec<_>>(),
            vec![GamepadBackendKind::Auto, GamepadBackendKind::Gilrs]
        );
        let mut config = crate::config::app_config::AppConfig::default().input;
        config.backend = InputBackendKind::RawInput;
        config.gamepad_backend = GamepadBackendKind::RawInput;
        config.gamepad_slot_device_ids = [Some("test:pad".into()), None];
        let expected = GlobalInputConfig {
            backend: InputBackendKind::Auto,
            gamepad_backend: GamepadBackendKind::Gilrs,
            ..config.clone()
        };
        available.normalize_config(&mut config);
        assert_eq!(toml::to_string(&config).unwrap(), toml::to_string(&expected).unwrap());
    }
}
