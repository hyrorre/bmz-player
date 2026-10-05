pub(crate) mod availability;
pub mod capture;
#[cfg(target_os = "macos")]
pub mod gamecontroller;
#[cfg(all(windows, feature = "experimental-gameinput"))]
pub mod gameinput;
pub mod gamepad;
pub mod gilrs;
#[cfg(all(target_os = "linux", feature = "linux-evdev"))]
mod linux_clock;
#[cfg(all(target_os = "linux", feature = "linux-evdev"))]
pub mod linux_evdev;
#[cfg(all(target_os = "linux", feature = "linux-evdev"))]
mod linux_keys;
#[cfg(all(target_os = "macos", feature = "macos-iohid"))]
pub mod macos;
#[cfg(any(target_os = "macos", test))]
mod macos_clock;
#[cfg(any(target_os = "macos", test))]
mod macos_keys;
#[cfg(windows)]
mod native_capture;
pub mod rawinput;
pub mod shared;
pub mod winit;
