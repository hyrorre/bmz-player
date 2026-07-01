use std::env;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

pub const RESOURCE_PATH_PREFIX: &str = "resource:";
pub const DATA_PATH_PREFIX: &str = "data:";

#[cfg(any(target_os = "windows", target_os = "macos"))]
const APP_DIR_NAME: &str = "BMZ Player";
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
const UNIX_APP_DIR_NAME: &str = "bmz-player";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub resource_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub config_toml: PathBuf,
    pub library_db: PathBuf,
    pub profiles_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProfilePaths {
    pub root_dir: PathBuf,
    pub profile_toml: PathBuf,
    pub collection_db: PathBuf,
    pub score_db: PathBuf,
    pub replay_dir: PathBuf,
}

pub fn resolve_app_paths() -> Result<AppPaths> {
    let current_dir = env::current_dir().context("failed to resolve current directory")?;
    let exe_path = env::current_exe().ok();
    let exe_dir = exe_path.as_ref().and_then(|path| path.parent()).map(Path::to_path_buf);

    let resource_dir = env_path("BMZ_RESOURCE_DIR").unwrap_or_else(|| {
        default_resource_dir(&current_dir, exe_path.as_deref(), exe_dir.as_deref())
    });
    let data_dir_overridden = env_path("BMZ_DATA_DIR");
    let data_dir = data_dir_overridden
        .clone()
        .unwrap_or_else(|| default_data_dir(&current_dir, exe_dir.as_deref()));
    let cache_dir = env_path("BMZ_CACHE_DIR").unwrap_or_else(|| {
        default_cache_dir(
            &current_dir,
            exe_dir.as_deref(),
            &data_dir,
            data_dir_overridden.is_some(),
        )
    });
    let logs_dir = env_path("BMZ_LOGS_DIR").unwrap_or_else(|| {
        default_logs_dir(&current_dir, exe_dir.as_deref(), &data_dir, data_dir_overridden.is_some())
    });

    Ok(AppPaths::from_dirs(resource_dir, data_dir, cache_dir, logs_dir))
}

pub fn resolve_profile_paths(app: &AppPaths, profile_id: &str) -> Result<ProfilePaths> {
    validate_profile_id(profile_id)?;
    let root_dir = app.profiles_dir.join(profile_id);
    Ok(ProfilePaths {
        profile_toml: root_dir.join("profile.toml"),
        collection_db: root_dir.join("collection.db"),
        score_db: root_dir.join("score.db"),
        replay_dir: root_dir.join("replay"),
        root_dir,
    })
}

pub fn validate_profile_id(profile_id: &str) -> Result<()> {
    if profile_id.is_empty() {
        bail!("profile id must not be empty");
    }

    if profile_id.len() > 64 {
        bail!("profile id must be 64 bytes or less");
    }

    if !profile_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("profile id may only contain ASCII letters, digits, '_' and '-'");
    }

    Ok(())
}

impl AppPaths {
    pub fn from_dirs(
        resource_dir: PathBuf,
        data_dir: PathBuf,
        cache_dir: PathBuf,
        logs_dir: PathBuf,
    ) -> Self {
        Self {
            config_toml: data_dir.join("config.toml"),
            library_db: data_dir.join("library.db"),
            profiles_dir: data_dir.join("profiles"),
            resource_dir,
            data_dir,
            cache_dir,
            logs_dir,
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.data_dir.join("skins"))?;
        std::fs::create_dir_all(&self.profiles_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(&self.logs_dir)?;
        Ok(())
    }

    pub fn default_skin_root(&self) -> PathBuf {
        self.resource_dir.join("skins/default")
    }

    pub fn hides_bundled_skin_label(&self) -> bool {
        same_path(&self.resource_dir.join("skins"), &self.data_dir.join("skins"))
            || sibling_named_dirs(&self.resource_dir, "resources", &self.data_dir, "data")
    }

    pub fn resolve_path_ref(&self, path_ref: &str) -> Result<PathBuf> {
        let trimmed = path_ref.trim();
        if let Some(relative) = trimmed.strip_prefix(RESOURCE_PATH_PREFIX) {
            return join_checked(&self.resource_dir, relative);
        }
        if let Some(relative) = trimmed.strip_prefix(DATA_PATH_PREFIX) {
            return join_checked(&self.data_dir, relative);
        }

        let path = Path::new(trimmed);
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        if let Some(relative) = strip_first_component(path, "data") {
            if let Some(skin_relative) = strip_first_component(&relative, "skins") {
                let data_candidate = self.data_dir.join("skins").join(&skin_relative);
                if data_candidate.exists() {
                    return Ok(data_candidate);
                }
                let resource_candidate = self.resource_dir.join("skins").join(&skin_relative);
                if resource_candidate.exists() {
                    return Ok(resource_candidate);
                }
                return Ok(data_candidate);
            }
            return Ok(self.data_dir.join(relative));
        }
        Ok(path.to_path_buf())
    }

    pub fn resolve_optional_path_ref(&self, path_ref: &str) -> Result<Option<PathBuf>> {
        if path_ref.trim().is_empty() {
            return Ok(None);
        }
        self.resolve_path_ref(path_ref).map(Some)
    }

    pub fn resource_path_ref(&self, path: &Path) -> Option<String> {
        path.strip_prefix(&self.resource_dir)
            .ok()
            .map(|relative| format!("{RESOURCE_PATH_PREFIX}{}", path_to_slash(relative)))
    }

    pub fn data_path_ref(&self, path: &Path) -> Option<String> {
        path.strip_prefix(&self.data_dir)
            .ok()
            .map(|relative| format!("{DATA_PATH_PREFIX}{}", path_to_slash(relative)))
    }
}

impl ProfilePaths {
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root_dir)?;
        std::fs::create_dir_all(&self.replay_dir)?;
        Ok(())
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn default_resource_dir(
    current_dir: &Path,
    exe_path: Option<&Path>,
    exe_dir: Option<&Path>,
) -> PathBuf {
    if let Some(resources) = macos_app_resource_dir(exe_path).filter(|path| path.exists()) {
        return resources;
    }
    if let Some(resources) = exe_dir.map(|dir| dir.join("resources")).filter(|path| path.exists()) {
        return resources;
    }

    let repo_data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    if repo_data.exists() {
        return repo_data;
    }

    let current_data = current_dir.join("data");
    if current_data.exists() {
        return current_data;
    }

    macos_app_resource_dir(exe_path)
        .or_else(|| exe_dir.map(|dir| dir.join("resources")))
        .unwrap_or(current_data)
}

fn default_data_dir(current_dir: &Path, exe_dir: Option<&Path>) -> PathBuf {
    if let Some(data_dir) = exe_dir.map(|dir| dir.join("data")).filter(|path| path.exists()) {
        return data_dir;
    }

    let current_data = current_dir.join("data");
    if current_data.exists() {
        return current_data;
    }

    platform_data_dir().unwrap_or(current_data)
}

fn default_cache_dir(
    current_dir: &Path,
    exe_dir: Option<&Path>,
    data_dir: &Path,
    data_dir_overridden: bool,
) -> PathBuf {
    if data_dir_overridden || is_portable_data_dir(current_dir, exe_dir, data_dir) {
        return data_dir.join("cache");
    }
    platform_cache_dir().unwrap_or_else(|| data_dir.join("cache"))
}

fn default_logs_dir(
    current_dir: &Path,
    exe_dir: Option<&Path>,
    data_dir: &Path,
    data_dir_overridden: bool,
) -> PathBuf {
    if data_dir_overridden || is_portable_data_dir(current_dir, exe_dir, data_dir) {
        return data_dir.join("logs");
    }
    platform_logs_dir().unwrap_or_else(|| data_dir.join("logs"))
}

fn is_portable_data_dir(current_dir: &Path, exe_dir: Option<&Path>, data_dir: &Path) -> bool {
    if data_dir == current_dir.join("data") {
        return true;
    }
    exe_dir.map(|dir| dir.join("data")).is_some_and(|path| path == data_dir)
}

fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn sibling_named_dirs(a: &Path, a_name: &str, b: &Path, b_name: &str) -> bool {
    path_file_name_eq(a, a_name)
        && path_file_name_eq(b, b_name)
        && a.parent()
            .zip(b.parent())
            .is_some_and(|(a_parent, b_parent)| same_path(a_parent, b_parent))
}

fn path_file_name_eq(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn macos_app_resource_dir(exe_path: Option<&Path>) -> Option<PathBuf> {
    let exe_path = exe_path?;
    let macos_dir = exe_path.parent()?;
    if macos_dir.file_name()? != "MacOS" {
        return None;
    }
    let contents_dir = macos_dir.parent()?;
    if contents_dir.file_name()? != "Contents" {
        return None;
    }
    Some(contents_dir.join("Resources"))
}

#[cfg(target_os = "windows")]
fn platform_data_dir() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|base| PathBuf::from(base).join(APP_DIR_NAME))
}

#[cfg(target_os = "macos")]
fn platform_data_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support").join(APP_DIR_NAME))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn platform_data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|base| base.join(UNIX_APP_DIR_NAME))
}

#[cfg(target_os = "windows")]
fn platform_cache_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join(APP_DIR_NAME).join("cache"))
}

#[cfg(target_os = "macos")]
fn platform_cache_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Caches").join(APP_DIR_NAME))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn platform_cache_dir() -> Option<PathBuf> {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|base| base.join(UNIX_APP_DIR_NAME))
}

#[cfg(target_os = "windows")]
fn platform_logs_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join(APP_DIR_NAME).join("logs"))
}

#[cfg(target_os = "macos")]
fn platform_logs_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from).map(|home| home.join("Library/Logs").join(APP_DIR_NAME))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn platform_logs_dir() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|base| base.join(UNIX_APP_DIR_NAME).join("logs"))
}

fn join_checked(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("path reference must stay under its root: {relative}");
            }
        }
    }
    Ok(path)
}

fn strip_first_component(path: &Path, expected: &str) -> Option<PathBuf> {
    let mut components = path.components();
    match components.next()? {
        Component::Normal(first) if first == std::ffi::OsStr::new(expected) => {
            Some(components.as_path().to_path_buf())
        }
        _ => None,
    }
}

fn path_to_slash(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app_paths() -> AppPaths {
        AppPaths::from_dirs(
            PathBuf::from("resources"),
            PathBuf::from("data"),
            PathBuf::from("data/cache"),
            PathBuf::from("data/logs"),
        )
    }

    #[test]
    fn profile_paths_are_rooted_under_profiles_dir() {
        let app = test_app_paths();

        let paths = resolve_profile_paths(&app, "default-1").unwrap();

        assert_eq!(paths.root_dir, PathBuf::from("data/profiles/default-1"));
        assert_eq!(paths.collection_db, PathBuf::from("data/profiles/default-1/collection.db"));
        assert_eq!(paths.score_db, PathBuf::from("data/profiles/default-1/score.db"));
    }

    #[test]
    fn profile_id_rejects_path_traversal() {
        assert!(validate_profile_id("../default").is_err());
        assert!(validate_profile_id("profile/name").is_err());
        assert!(validate_profile_id("").is_err());
        assert!(validate_profile_id("default_1-2").is_ok());
    }

    #[test]
    fn ensure_dirs_creates_user_skin_root() {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = env::temp_dir()
            .join(format!("bmz-player-paths-ensure-dirs-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = AppPaths::from_dirs(
            root.join("resources"),
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        );

        app.ensure_dirs().unwrap();

        assert!(root.join("data/skins").is_dir());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_refs_resolve_against_resource_and_data_roots() {
        let app = test_app_paths();

        assert_eq!(
            app.resolve_path_ref("resource:skins/Rmz-skin/play7main.luaskin").unwrap(),
            PathBuf::from("resources/skins/Rmz-skin/play7main.luaskin")
        );
        assert_eq!(
            app.resolve_path_ref("data:skins/custom/play7.luaskin").unwrap(),
            PathBuf::from("data/skins/custom/play7.luaskin")
        );
        assert_eq!(
            app.resolve_path_ref("data/skins/legacy/play7.luaskin").unwrap(),
            PathBuf::from("data/skins/legacy/play7.luaskin")
        );
    }

    #[test]
    fn bundled_skin_label_is_hidden_for_shared_development_skin_root() {
        let app = AppPaths::from_dirs(
            PathBuf::from("data"),
            PathBuf::from("data"),
            PathBuf::from("data/cache"),
            PathBuf::from("data/logs"),
        );

        assert!(app.hides_bundled_skin_label());
    }

    #[test]
    fn bundled_skin_label_is_hidden_for_portable_sibling_data_layout() {
        let root = PathBuf::from("portable");
        let app = AppPaths::from_dirs(
            root.join("resources"),
            root.join("data"),
            root.join("data/cache"),
            root.join("data/logs"),
        );

        assert!(app.hides_bundled_skin_label());
    }

    #[test]
    fn bundled_skin_label_is_kept_for_separate_user_data_layout() {
        let root = PathBuf::from("installed");
        let app = AppPaths::from_dirs(
            root.join("resources"),
            PathBuf::from("profile-data"),
            PathBuf::from("profile-data/cache"),
            PathBuf::from("profile-data/logs"),
        );

        assert!(!app.hides_bundled_skin_label());
    }

    #[test]
    fn legacy_data_skin_paths_fall_back_to_bundled_skin_when_user_copy_is_missing() {
        let root =
            env::temp_dir().join(format!("bmz-player-paths-legacy-skin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let resource_skin = root.join("resources/skins/Rmz-skin/play7main.luaskin");
        std::fs::create_dir_all(resource_skin.parent().unwrap()).unwrap();
        std::fs::write(&resource_skin, b"return {}").unwrap();
        let app = AppPaths::from_dirs(
            root.join("resources"),
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        );

        assert_eq!(
            app.resolve_path_ref("data/skins/Rmz-skin/play7main.luaskin").unwrap(),
            resource_skin
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_refs_reject_root_escape() {
        let app = test_app_paths();

        assert!(app.resolve_path_ref("resource:../profile.toml").is_err());
        assert!(app.resolve_path_ref("data:/absolute").is_err());
    }
}
