//! Lane cover (Qwilight "veil") images added from the skin settings.
//!
//! Qwilight keeps the player's own veil images in its configuration. BMZ copies
//! them below the user skin root, so every Qwilight play skin can read them
//! within its sandbox, and selects one with the skin's "Lane cover image" option.
use super::*;

/// Larger files are rejected before copying; the decoder applies its own limits.
const LANE_COVER_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Set when BMZ adds or removes a lane cover. The app then drops the evaluated
/// Qwilight definitions, whose lane cover items list the images on disk.
pub(in crate::ui) const QWILIGHT_LANE_COVERS_CHANGED_ID: &str = "qwilight_lane_covers_changed";

pub(in crate::ui) fn qwilight_lane_cover_dir(app_paths: &AppPaths) -> PathBuf {
    app_paths.data_dir.join("skins").join(bmz_skin::QWILIGHT_LANE_COVER_DIR)
}

/// Copies images into `dir` and returns their names there, in the given order.
/// An identical file already present is reused; another with the same name is kept.
pub(in crate::ui) fn add_qwilight_lane_covers(
    dir: &Path,
    sources: &[PathBuf],
) -> Result<Vec<String>, String> {
    fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut names = Vec::with_capacity(sources.len());
    for source in sources {
        let fail = |error: &dyn std::fmt::Display| format!("{}: {error}", source.display());
        if !bmz_skin::is_qwilight_lane_cover_image_path(source) {
            return Err(fail(&"unsupported image format"));
        }
        let metadata = fs::metadata(source).map_err(|error| fail(&error))?;
        if !metadata.is_file() || metadata.len() > LANE_COVER_MAX_BYTES {
            return Err(fail(&"not an image file up to 32 MiB"));
        }
        let bytes = fs::read(source).map_err(|error| fail(&error))?;
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| fail(&"file name is not valid Unicode"))?;
        let (stem, extension) = file_name.rsplit_once('.').unwrap_or((file_name, ""));
        let mut name = file_name.to_string();
        for copy in 2.. {
            let target = dir.join(&name);
            match fs::read(&target) {
                Ok(existing) if existing == bytes => break,
                Ok(_) => name = format!("{stem} ({copy}).{extension}"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::write(&target, &bytes).map_err(|error| fail(&error))?;
                    break;
                }
                Err(error) => return Err(fail(&error)),
            }
        }
        names.push(name);
    }
    Ok(names)
}

/// Deletes BMZ's copy. Names other than a plain file in `dir` are rejected.
pub(in crate::ui) fn remove_qwilight_lane_cover(dir: &Path, name: &str) -> Result<(), String> {
    let path = qwilight_lane_cover_file(dir, name).ok_or_else(|| format!("{name}: not found"))?;
    fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))
}

fn qwilight_lane_cover_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let plain = Path::new(name).file_name().and_then(|n| n.to_str()) == Some(name);
    let path = dir.join(name);
    (plain && path.is_file()).then_some(path)
}

/// Add / remove / open buttons below the options. Returns whether the
/// slot's options changed, which reloads the skin like any option change.
pub(in crate::ui) fn build_qwilight_lane_cover_tools(
    ui: &mut egui::Ui,
    slot: SkinSlot,
    app_paths: &AppPaths,
    options: &mut BTreeMap<String, String>,
    text: Localizer,
) -> bool {
    let dir = qwilight_lane_cover_dir(app_paths);
    let error_id = egui::Id::new(("qwilight_lane_cover_error", slot.defs_header_id()));
    let property = bmz_skin::QWILIGHT_LANE_COVER_PROPERTY;
    let selected_file = options
        .get(property)
        .and_then(|name| qwilight_lane_cover_file(&dir, name).map(|_| name.clone()));
    let mut changed = false;
    let mut error = None;
    ui.label(tr!(text, "skin-qwilight-lane-cover-help"));
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr!(text, "skin-qwilight-lane-cover-add")).clicked()
            && let Some(sources) = rfd::FileDialog::new()
                .add_filter(
                    tr!(text, "skin-qwilight-lane-cover-filter"),
                    &["png", "jpg", "jpeg", "bmp", "webp"],
                )
                .pick_files()
        {
            match add_qwilight_lane_covers(&dir, &sources) {
                Ok(names) => {
                    if let Some(name) = names.last() {
                        options.insert(property.to_string(), name.clone());
                        changed = true;
                    }
                }
                Err(message) => error = Some(message),
            }
        }
        if ui
            .add_enabled(
                selected_file.is_some(),
                egui::Button::new(tr!(text, "skin-qwilight-lane-cover-remove")),
            )
            .clicked()
            && let Some(name) = &selected_file
        {
            match remove_qwilight_lane_cover(&dir, name) {
                Ok(()) => {
                    // The first item is the skin's own lane cover.
                    options.remove(property);
                    changed = true;
                }
                Err(message) => error = Some(message),
            }
        }
        if ui.button(tr!(text, "skin-qwilight-lane-cover-open")).clicked() {
            error = fs::create_dir_all(&dir)
                .map_err(|error| error.to_string())
                .and_then(|()| open_directory(&dir, text))
                .err();
        }
    });
    if changed {
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::new(QWILIGHT_LANE_COVERS_CHANGED_ID), true)
        });
    }
    if changed || error.is_some() {
        ui.ctx().data_mut(|data| data.insert_temp(error_id, error));
    }
    if let Some(error) =
        ui.ctx().data_mut(|data| data.get_temp::<Option<String>>(error_id)).flatten()
    {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    changed
}
