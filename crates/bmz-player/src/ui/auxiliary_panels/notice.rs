use super::*;

pub(super) fn build_third_party_notice_page(
    ui: &mut egui::Ui,
    app_paths: &AppPaths,
    notice_text: &mut Option<String>,
) {
    if SettingsNavigation::load(ui.ctx()).page != SettingsPage::Licenses {
        return;
    }
    let notice = notice_text.get_or_insert_with(|| combined_license_notice_text(app_paths));
    let mut notice = notice.as_str();
    ui.add(
        egui::TextEdit::multiline(&mut notice)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .interactive(false),
    );
}

pub(super) fn combined_license_notice_text(app_paths: &AppPaths) -> String {
    combined_license_notice_text_with_repo_root(app_paths, &repo_root())
}

pub(super) fn combined_license_notice_text_with_repo_root(
    app_paths: &AppPaths,
    repo_root: &Path,
) -> String {
    let third_party = third_party_notice_text(app_paths);
    let rust_dependencies = rust_dependency_license_text(app_paths, repo_root);

    format!(
        "{third_party}\n\n\n================================================================\nGenerated Rust Dependency License Report\n================================================================\n\n{rust_dependencies}"
    )
}

pub(super) fn third_party_notice_text(app_paths: &AppPaths) -> String {
    let packaged = app_paths.resource_dir.join(THIRD_PARTY_NOTICE_PATH);
    read_non_empty_text(&packaged).unwrap_or_else(|| BUNDLED_THIRD_PARTY_NOTICES.to_string())
}

pub(super) fn rust_dependency_license_text(app_paths: &AppPaths, repo_root: &Path) -> String {
    let packaged = app_paths.resource_dir.join(RUST_DEPENDENCY_LICENSE_PATH);
    if let Some(text) = read_non_empty_text(&packaged) {
        return text;
    }

    let local = repo_root.join(LOCAL_RUST_DEPENDENCY_LICENSE_FILE);
    if let Some(text) = read_non_empty_text(&local) {
        return text;
    }

    missing_rust_dependency_license_text(&packaged, &local)
}

pub(super) fn read_non_empty_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().filter(|text| !text.trim().is_empty())
}

pub(super) fn missing_rust_dependency_license_text(packaged: &Path, local: &Path) -> String {
    format!(
        "BMZ Player Rust Dependency Licenses\n===================================\n\nThe generated Rust dependency license report was not found.\n\nExpected packaged path:\n  {}\n\nLocal development fallback:\n  {}\n\nGenerate it from the repository root with:\n\n  cargo-about generate --workspace --locked --fail \\\n    --output-file rust-dependency-licenses.txt \\\n    about.hbs\n",
        packaged.display(),
        local.display()
    )
}

pub(super) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(super) fn directory_open_targets(app_paths: &AppPaths) -> [DirectoryOpenTarget<'_>; 4] {
    [
        DirectoryOpenTarget { label: "resource_dir", path: &app_paths.resource_dir },
        DirectoryOpenTarget { label: "data_dir", path: &app_paths.data_dir },
        DirectoryOpenTarget { label: "cache_dir", path: &app_paths.cache_dir },
        DirectoryOpenTarget { label: "logs_dir", path: &app_paths.logs_dir },
    ]
}

pub(super) fn open_directory_target(
    target: DirectoryOpenTarget<'_>,
    text: Localizer,
) -> DirectoryOpenStatus {
    let error = open_directory(target.path, text).err();
    DirectoryOpenStatus { label: target.label, path: target.path.to_path_buf(), error }
}

pub(super) fn open_directory(path: &Path, text: Localizer) -> Result<(), String> {
    if !path.is_dir() {
        return Err(tr!(
            text,
            "menu-directory-missing",
            "path" => path.display().to_string()
        ));
    }
    spawn_directory_opener(path).map_err(|error| format!("{} ({})", error, path.display()))
}

#[cfg(target_os = "macos")]
pub(super) fn spawn_directory_opener(path: &Path) -> std::io::Result<()> {
    run_directory_opener("open", path)
}

#[cfg(target_os = "windows")]
pub(super) fn spawn_directory_opener(path: &Path) -> std::io::Result<()> {
    // explorer.exe may hand the request to the existing shell process and
    // return a non-zero status even though the directory was opened.
    Command::new("explorer").arg(path).spawn().map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) fn spawn_directory_opener(path: &Path) -> std::io::Result<()> {
    run_directory_opener("xdg-open", path)
}

#[cfg(unix)]
pub(super) fn run_directory_opener(program: &str, path: &Path) -> std::io::Result<()> {
    let status = crate::external_process::desktop_command(program).arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("{program} exited with {status}")))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
pub(super) fn spawn_directory_opener(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "opening directories is not supported on this platform",
    ))
}
