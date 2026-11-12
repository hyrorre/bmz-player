use super::*;

#[path = "select/arrange_controls.rs"]
mod arrange_controls;
#[path = "select/cases_01.rs"]
mod cases_01;
#[path = "select/cases_02.rs"]
mod cases_02;
#[path = "select/cases_03.rs"]
mod cases_03;
#[path = "select/cases_04.rs"]
mod cases_04;
#[path = "select/cases_05.rs"]
mod cases_05;
#[path = "select/lr2_buttons.rs"]
mod lr2_buttons;
#[path = "select/navigation.rs"]
mod navigation;
#[path = "select/session_mode.rs"]
mod session_mode;
#[cfg(target_os = "windows")]
#[path = "select/settings_controls.rs"]
mod settings_controls;
