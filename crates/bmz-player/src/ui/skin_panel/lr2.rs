//! Skin settings embedded in LR2 select skins (buttons 170..190 / 220..229,
//! text 50 / 51 / 100..119, slider 7). They edit the same profile skin settings as
//! the F1 settings screen.

/// An LR2 skin type with a BMZ skin slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Lr2SkinTarget(SkinSlot);

impl Default for Lr2SkinTarget {
    fn default() -> Self {
        Self(SkinSlot::Play7)
    }
}

impl Lr2SkinTarget {
    /// LR2 buttons 170..185. KEYCONFIG, SKINSELECT, SOUNDSET, THEME and BATTLE9
    /// have no BMZ skin slot.
    pub(crate) fn from_button(button: i32) -> Option<Self> {
        Some(Self(match button {
            170 => SkinSlot::Play7,
            171 => SkinSlot::Play5,
            172 => SkinSlot::Play14,
            173 => SkinSlot::Play10,
            174 => SkinSlot::Play9,
            175 => SkinSlot::Select,
            176 => SkinSlot::Decide,
            177 => SkinSlot::Result,
            182 => SkinSlot::Battle7,
            183 => SkinSlot::Battle5,
            185 => SkinSlot::CourseResult,
            _ => return None,
        }))
    }

    pub(crate) fn button(self) -> i32 {
        match self.0 {
            SkinSlot::Play7 => 170,
            SkinSlot::Play5 => 171,
            SkinSlot::Play14 => 172,
            SkinSlot::Play10 => 173,
            SkinSlot::Play9 => 174,
            SkinSlot::Select => 175,
            SkinSlot::Decide => 176,
            SkinSlot::Result => 177,
            SkinSlot::Battle7 => 182,
            SkinSlot::Battle5 => 183,
            SkinSlot::CourseResult => 185,
            SkinSlot::Play4 | SkinSlot::Play6 | SkinSlot::Play8 => 0,
        }
    }

    pub(crate) fn is_play(self) -> bool {
        !matches!(
            self.0,
            SkinSlot::Select | SkinSlot::Decide | SkinSlot::Result | SkinSlot::CourseResult
        )
    }

    /// The slot's path, options and files; the customize list changes with them.
    pub(crate) fn settings_key(
        self,
        skin: &SkinConfig,
    ) -> (String, BTreeMap<String, String>, BTreeMap<String, String>) {
        (
            skin_slot_path(skin, self.0).to_string(),
            slot_options(skin, self.0).clone(),
            slot_files(skin, self.0).clone(),
        )
    }

    pub(crate) fn reload_request(self) -> SkinReloadRequest {
        let mut request = SkinReloadRequest::default();
        request_skin_reload(&mut request, self.0, false);
        request
    }

    /// Steps through BMZ's default skin and the installed candidates, wrapping
    /// around like LR2. Options of each skin are kept in the profile history.
    pub(crate) fn cycle_skin(
        self,
        skin: &mut SkinConfig,
        catalog: &SkinCatalog,
        direction: i32,
    ) -> bool {
        let candidates = slot_candidates(catalog, self.0);
        let paths: Vec<&str> =
            std::iter::once("").chain(candidates.iter().map(|c| c.path.as_str())).collect();
        let current = skin_slot_path(skin, self.0);
        let index = paths.iter().position(|path| *path == current).unwrap_or(0);
        let next = paths[step_index(index, paths.len(), direction)].to_string();
        if next == current {
            return false;
        }
        save_skin_slot_history(skin, self.0);
        *skin_slot_path_mut(skin, self.0) = next;
        restore_skin_slot_history(skin, self.0);
        true
    }
}

fn slot_candidates(catalog: &SkinCatalog, slot: SkinSlot) -> &[SkinCandidate] {
    match slot {
        SkinSlot::Select => &catalog.select,
        SkinSlot::Decide => &catalog.decide,
        SkinSlot::Play4 => &catalog.play4,
        SkinSlot::Play5 => &catalog.play5,
        SkinSlot::Play6 => &catalog.play6,
        SkinSlot::Play7 => &catalog.play7,
        SkinSlot::Play8 => &catalog.play8,
        SkinSlot::Play9 => &catalog.play9,
        SkinSlot::Play10 => &catalog.play10,
        SkinSlot::Play14 => &catalog.play14,
        SkinSlot::Battle5 => &catalog.battle5,
        SkinSlot::Battle7 => &catalog.battle7,
        SkinSlot::Result => &catalog.result,
        SkinSlot::CourseResult => &catalog.course_result,
    }
}

fn slot_options(skin: &SkinConfig, slot: SkinSlot) -> &BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &skin.select_options,
        SkinSlot::Decide => &skin.decide_options,
        SkinSlot::Play4 => &skin.play4_options,
        SkinSlot::Play5 => &skin.play5_options,
        SkinSlot::Play6 => &skin.play6_options,
        SkinSlot::Play7 => &skin.play7_options,
        SkinSlot::Play8 => &skin.play8_options,
        SkinSlot::Play9 => &skin.play9_options,
        SkinSlot::Play10 => &skin.play10_options,
        SkinSlot::Play14 => &skin.play14_options,
        SkinSlot::Battle5 => &skin.battle5_options,
        SkinSlot::Battle7 => &skin.battle7_options,
        SkinSlot::Result => &skin.result_options,
        SkinSlot::CourseResult => &skin.course_result_options,
    }
}

fn slot_files(skin: &SkinConfig, slot: SkinSlot) -> &BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &skin.select_files,
        SkinSlot::Decide => &skin.decide_files,
        SkinSlot::Play4 => &skin.play4_files,
        SkinSlot::Play5 => &skin.play5_files,
        SkinSlot::Play6 => &skin.play6_files,
        SkinSlot::Play7 => &skin.play7_files,
        SkinSlot::Play8 => &skin.play8_files,
        SkinSlot::Play9 => &skin.play9_files,
        SkinSlot::Play10 => &skin.play10_files,
        SkinSlot::Play14 => &skin.play14_files,
        SkinSlot::Battle5 => &skin.battle5_files,
        SkinSlot::Battle7 => &skin.battle7_files,
        SkinSlot::Result => &skin.result_files,
        SkinSlot::CourseResult => &skin.course_result_files,
    }
}

fn step_index(index: usize, len: usize, direction: i32) -> usize {
    if direction >= 0 { (index + 1) % len } else { (index + len - 1) % len }
}

/// One row of LR2's customize list: an option, then the file selections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lr2SkinItem {
    pub(crate) name: String,
    pub(crate) value: String,
}

/// The customize list of `target`'s current skin with the saved values,
/// falling back to the skin's defaults like the settings screen.
pub(crate) fn lr2_skin_items(
    target: Lr2SkinTarget,
    defs: &SceneSkinDefs,
    skin: &SkinConfig,
    app_paths: &AppPaths,
) -> Vec<Lr2SkinItem> {
    let options = slot_options(skin, target.0);
    let files = slot_files(skin, target.0);
    let context = (!defs.filepath.is_empty())
        .then(|| skin_root_path(app_paths, skin_slot_path(skin, target.0)))
        .flatten();
    let properties = defs.property.iter().map(|prop| Lr2SkinItem {
        name: prop.name.clone(),
        value: options.get(&prop.name).cloned().unwrap_or_else(|| property_default(prop)),
    });
    let file_items = defs.filepath.iter().map(|filepath| {
        let selected = files.get(&filepath.name).cloned().or_else(|| {
            let candidates =
                context.as_ref().map(|c| glob_candidates_for_skin(c, &filepath.path))?;
            filepath_default(filepath, &candidates)
        });
        Lr2SkinItem { name: filepath.name.clone(), value: file_value_label(selected.as_deref()) }
    });
    properties.chain(file_items).collect()
}

fn file_value_label(selected: Option<&str>) -> String {
    match selected {
        None | Some("") => String::new(),
        Some(RANDOM_FILE_SELECTION) => "RANDOM".to_string(),
        Some(value) => filepath_selection_label(value).to_string(),
    }
}

/// Steps the value of customize row `index`, wrapping around like LR2.
/// File selections step through RANDOM and the matching files.
pub(crate) fn lr2_cycle_skin_item(
    target: Lr2SkinTarget,
    defs: &SceneSkinDefs,
    skin: &mut SkinConfig,
    app_paths: &AppPaths,
    index: usize,
    direction: i32,
) -> bool {
    if let Some(prop) = defs.property.get(index) {
        if prop.item.is_empty() {
            return false;
        }
        let options = skin_slot_options_mut(skin, target.0);
        let current = options.get(&prop.name).cloned().unwrap_or_else(|| property_default(prop));
        let position = prop.item.iter().position(|item| item.name == current).unwrap_or(0);
        let next = prop.item[step_index(position, prop.item.len(), direction)].name.clone();
        return options.insert(prop.name.clone(), next.clone()).as_deref() != Some(next.as_str());
    }
    let Some(filepath) = defs.filepath.get(index - defs.property.len()) else {
        return false;
    };
    let Some(context) = skin_root_path(app_paths, skin_slot_path(skin, target.0)) else {
        return false;
    };
    let candidates = glob_candidates_for_skin(&context, &filepath.path);
    let values: Vec<String> = std::iter::once(RANDOM_FILE_SELECTION.to_string())
        .chain(candidates.iter().cloned())
        .collect();
    let files = skin_slot_files_mut(skin, target.0);
    let current = files
        .get(&filepath.name)
        .cloned()
        .or_else(|| filepath_default(filepath, &candidates))
        .unwrap_or_default();
    let current = normalize_filepath_selection(&current, &candidates).unwrap_or(current);
    let position = values.iter().position(|value| *value == current).unwrap_or(0);
    let next = values[step_index(position, values.len(), direction)].clone();
    files.insert(filepath.name.clone(), next.clone()).as_deref() != Some(next.as_str())
}

use super::*;
