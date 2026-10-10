//! Desktop helpers started by BMZ must not inherit the Linux tar launcher's private
//! PipeWire search paths: a browser loading the bundled modules into the host
//! libpipewire can fail or crash on a version mismatch.
use std::ffi::{OsStr, OsString};
use std::process::Command;

/// Set by `installer/linux-tar/bmz-player` while it points these at bundled files.
const BUNDLED_MARKER: &str = "BMZ_BUNDLED_PIPEWIRE";
const BUNDLED_PIPEWIRE_ENV: [&str; 3] =
    ["PIPEWIRE_MODULE_DIR", "SPA_PLUGIN_DIR", "PIPEWIRE_CONFIG_DIR"];

/// A command for xdg-open, browsers and file managers, with the caller's environment.
pub(crate) fn desktop_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    restore_launcher_environment(&mut command, |key| std::env::var_os(key));
    command
}

fn restore_launcher_environment(command: &mut Command, var: impl Fn(&str) -> Option<OsString>) {
    if var(BUNDLED_MARKER).is_none() {
        return;
    }
    for key in BUNDLED_PIPEWIRE_ENV {
        let original = format!("BMZ_ORIG_{key}");
        match var(&original) {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
        command.env_remove(original);
    }
    command.env_remove(BUNDLED_MARKER);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn envs(command: &Command) -> HashMap<String, Option<String>> {
        command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn launcher_paths_are_removed_or_restored_for_children() {
        let parent = HashMap::from([
            (BUNDLED_MARKER, "1"),
            ("PIPEWIRE_MODULE_DIR", "/opt/bmz/lib/pipewire-0.3"),
            ("SPA_PLUGIN_DIR", "/opt/bmz/lib/spa-0.2"),
            ("PIPEWIRE_CONFIG_DIR", "/home/user/pw"),
            ("BMZ_ORIG_PIPEWIRE_CONFIG_DIR", "/home/user/pw"),
        ]);
        let mut command = Command::new("xdg-open");
        restore_launcher_environment(&mut command, |key| parent.get(key).map(OsString::from));
        let envs = envs(&command);
        assert_eq!(envs["PIPEWIRE_MODULE_DIR"], None);
        assert_eq!(envs["SPA_PLUGIN_DIR"], None);
        assert_eq!(envs["PIPEWIRE_CONFIG_DIR"].as_deref(), Some("/home/user/pw"));
        assert_eq!(envs[BUNDLED_MARKER], None);
    }

    #[test]
    fn environment_is_untouched_without_the_launcher() {
        let mut command = Command::new("xdg-open");
        restore_launcher_environment(&mut command, |_| None);
        assert!(envs(&command).is_empty());
    }
}
