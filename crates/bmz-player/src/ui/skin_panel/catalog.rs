pub(in crate::ui) fn skin_path_combo(
    ui: &mut egui::Ui,
    skin: &mut SkinConfig,
    slot: SkinSlot,
    app_paths: &AppPaths,
    candidates: &[SkinCandidate],
    show_bundled_origin: bool,
    text: Localizer,
) -> bool {
    let current = skin_slot_path(skin, slot).to_string();
    let mut selected = current.clone();
    let selected_text = skin_candidate_label(candidates, &current, show_bundled_origin, text);
    egui::ComboBox::from_id_salt(("skin_path_combo", slot.path_combo_id()))
        .selected_text(&selected_text)
        .width(ui.available_width().min(520.0))
        .truncate()
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, String::new(), tr!(text, "skin-default"));
            for candidate in candidates {
                let response = ui.selectable_value(
                    &mut selected,
                    candidate.path.clone(),
                    skin_candidate_display(candidate, show_bundled_origin, text),
                );
                if show_bundled_origin && candidate.origin == SkinCandidateOrigin::Bundled {
                    response.on_hover_text(tr!(text, "skin-origin-bundled-help"));
                }
            }
        })
        .response
        .on_hover_text(selected_text);
    switch_skin_slot(skin, slot, app_paths, selected)
}

pub(in crate::ui) fn skin_candidate_label(
    candidates: &[SkinCandidate],
    current: &str,
    show_bundled_origin: bool,
    text: Localizer,
) -> String {
    if current.is_empty() {
        return tr!(text, "skin-default");
    }
    candidates
        .iter()
        .find(|candidate| candidate.path == current)
        .map(|candidate| skin_candidate_display(candidate, show_bundled_origin, text))
        .unwrap_or_else(|| current.to_string())
}

pub(in crate::ui) fn skin_candidate_display(
    candidate: &SkinCandidate,
    show_bundled_origin: bool,
    text: Localizer,
) -> String {
    let label = skin_candidate_origin_label(candidate.origin, show_bundled_origin, text);
    let text = if candidate.name.is_empty() {
        candidate.path.clone()
    } else {
        format!("{} ({})", candidate.name, candidate.path)
    };
    if let Some(label) = label { format!("{label} {text}") } else { text }
}

pub(in crate::ui) fn skin_candidate_origin_label(
    origin: SkinCandidateOrigin,
    show_bundled_origin: bool,
    text: Localizer,
) -> Option<String> {
    match origin {
        SkinCandidateOrigin::Bundled if show_bundled_origin => {
            Some(tr!(text, "skin-origin-bundled"))
        }
        SkinCandidateOrigin::Bundled => None,
        SkinCandidateOrigin::User => Some(tr!(text, "skin-origin-user")),
        SkinCandidateOrigin::External => Some(tr!(text, "skin-origin-external")),
    }
}

pub(in crate::ui) fn show_bundled_skin_origin(
    app_paths: &AppPaths,
    skin_catalog: &SkinCatalog,
) -> bool {
    !app_paths.hides_bundled_skin_label() && skin_catalog_has_non_bundled_candidate(skin_catalog)
}

pub(in crate::ui) fn skin_catalog_has_non_bundled_candidate(skin_catalog: &SkinCatalog) -> bool {
    let groups: [&[SkinCandidate]; 14] = [
        &skin_catalog.select,
        &skin_catalog.decide,
        &skin_catalog.play4,
        &skin_catalog.play5,
        &skin_catalog.play6,
        &skin_catalog.play7,
        &skin_catalog.play8,
        &skin_catalog.play9,
        &skin_catalog.play10,
        &skin_catalog.play14,
        &skin_catalog.battle5,
        &skin_catalog.battle7,
        &skin_catalog.result,
        &skin_catalog.course_result,
    ];
    groups.iter().any(|candidates| {
        candidates.iter().any(|candidate| candidate.origin != SkinCandidateOrigin::Bundled)
    })
}

pub(in crate::ui) fn skin_slot_path(skin: &SkinConfig, slot: SkinSlot) -> &str {
    match slot {
        SkinSlot::Select => &skin.select,
        SkinSlot::Decide => &skin.decide,
        SkinSlot::Play4 => &skin.play4,
        SkinSlot::Play5 => &skin.play5,
        SkinSlot::Play6 => &skin.play6,
        SkinSlot::Play7 => &skin.play7,
        SkinSlot::Play8 => &skin.play8,
        SkinSlot::Play9 => &skin.play9,
        SkinSlot::Play10 => &skin.play10,
        SkinSlot::Play14 => &skin.play14,
        SkinSlot::Battle5 => &skin.battle5,
        SkinSlot::Battle7 => &skin.battle7,
        SkinSlot::Result => &skin.result,
        SkinSlot::CourseResult => &skin.course_result,
    }
}

pub(in crate::ui) fn skin_slot_path_mut(skin: &mut SkinConfig, slot: SkinSlot) -> &mut String {
    match slot {
        SkinSlot::Select => &mut skin.select,
        SkinSlot::Decide => &mut skin.decide,
        SkinSlot::Play4 => &mut skin.play4,
        SkinSlot::Play5 => &mut skin.play5,
        SkinSlot::Play6 => &mut skin.play6,
        SkinSlot::Play7 => &mut skin.play7,
        SkinSlot::Play8 => &mut skin.play8,
        SkinSlot::Play9 => &mut skin.play9,
        SkinSlot::Play10 => &mut skin.play10,
        SkinSlot::Play14 => &mut skin.play14,
        SkinSlot::Battle5 => &mut skin.battle5,
        SkinSlot::Battle7 => &mut skin.battle7,
        SkinSlot::Result => &mut skin.result,
        SkinSlot::CourseResult => &mut skin.course_result,
    }
}

pub(in crate::ui) fn skin_slot_options_mut(
    skin: &mut SkinConfig,
    slot: SkinSlot,
) -> &mut BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &mut skin.select_options,
        SkinSlot::Decide => &mut skin.decide_options,
        SkinSlot::Play4 => &mut skin.play4_options,
        SkinSlot::Play5 => &mut skin.play5_options,
        SkinSlot::Play6 => &mut skin.play6_options,
        SkinSlot::Play7 => &mut skin.play7_options,
        SkinSlot::Play8 => &mut skin.play8_options,
        SkinSlot::Play9 => &mut skin.play9_options,
        SkinSlot::Play10 => &mut skin.play10_options,
        SkinSlot::Play14 => &mut skin.play14_options,
        SkinSlot::Battle5 => &mut skin.battle5_options,
        SkinSlot::Battle7 => &mut skin.battle7_options,
        SkinSlot::Result => &mut skin.result_options,
        SkinSlot::CourseResult => &mut skin.course_result_options,
    }
}

pub(in crate::ui) fn skin_slot_files_mut(
    skin: &mut SkinConfig,
    slot: SkinSlot,
) -> &mut BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &mut skin.select_files,
        SkinSlot::Decide => &mut skin.decide_files,
        SkinSlot::Play4 => &mut skin.play4_files,
        SkinSlot::Play5 => &mut skin.play5_files,
        SkinSlot::Play6 => &mut skin.play6_files,
        SkinSlot::Play7 => &mut skin.play7_files,
        SkinSlot::Play8 => &mut skin.play8_files,
        SkinSlot::Play9 => &mut skin.play9_files,
        SkinSlot::Play10 => &mut skin.play10_files,
        SkinSlot::Play14 => &mut skin.play14_files,
        SkinSlot::Battle5 => &mut skin.battle5_files,
        SkinSlot::Battle7 => &mut skin.battle7_files,
        SkinSlot::Result => &mut skin.result_files,
        SkinSlot::CourseResult => &mut skin.course_result_files,
    }
}

pub(in crate::ui) fn skin_slot_offsets_mut(
    skin: &mut SkinConfig,
    slot: SkinSlot,
) -> &mut Vec<SkinOffsetConfig> {
    match slot {
        SkinSlot::Select => &mut skin.select_offsets,
        SkinSlot::Decide => &mut skin.decide_offsets,
        SkinSlot::Play4 => &mut skin.play4_offsets,
        SkinSlot::Play5 => &mut skin.play5_offsets,
        SkinSlot::Play6 => &mut skin.play6_offsets,
        SkinSlot::Play7 => &mut skin.play7_offsets,
        SkinSlot::Play8 => &mut skin.play8_offsets,
        SkinSlot::Play9 => &mut skin.play9_offsets,
        SkinSlot::Play10 => &mut skin.play10_offsets,
        SkinSlot::Play14 => &mut skin.play14_offsets,
        SkinSlot::Battle5 => &mut skin.battle5_offsets,
        SkinSlot::Battle7 => &mut skin.battle7_offsets,
        SkinSlot::Result => &mut skin.result_offsets,
        SkinSlot::CourseResult => &mut skin.course_result_offsets,
    }
}

pub(in crate::ui) fn skin_slot_history_key(slot: SkinSlot, path: &str) -> String {
    let slot_name = match slot {
        SkinSlot::Select => "select",
        SkinSlot::Decide => "decide",
        SkinSlot::Play4 => "play4",
        SkinSlot::Play5 => "play5",
        SkinSlot::Play6 => "play6",
        SkinSlot::Play7 => "play7",
        SkinSlot::Play8 => "play8",
        SkinSlot::Play9 => "play9",
        SkinSlot::Play10 => "play10",
        SkinSlot::Play14 => "play14",
        SkinSlot::Battle5 => "battle5",
        SkinSlot::Battle7 => "battle7",
        SkinSlot::Result => "result",
        SkinSlot::CourseResult => "course_result",
    };
    format!("{slot_name}::{path}")
}

pub(in crate::ui) fn switch_skin_slot(
    skin: &mut SkinConfig,
    slot: SkinSlot,
    app_paths: &AppPaths,
    next: String,
) -> bool {
    if skin_slot_path(skin, slot) == next {
        return false;
    }
    let current = skin_history_path(app_paths, skin, skin_slot_path(skin, slot));
    save_skin_slot_history(skin, slot, &current);
    *skin_slot_path_mut(skin, slot) = next;
    restore_skin_slot_history(skin, slot, app_paths);
    true
}

fn skin_history_path(app_paths: &AppPaths, skin: &SkinConfig, path: &str) -> String {
    let path = path.trim();
    let Some(resolved) = app_paths.resolve_optional_path_ref(path).ok().flatten() else {
        return path.to_owned();
    };
    let grant = if skin.external_access.is_empty() {
        None
    } else {
        app_paths.skin_access_key(&resolved).ok().and_then(|key| skin.external_access.get(&key))
    };
    let (entry, fallback) = if let Some(grant) = grant {
        // Keep the identity pinned to the approved file even when the link is unavailable.
        let entry = Path::new(&grant.entry);
        if !entry.is_absolute() {
            return path.to_owned();
        }
        (entry, grant.entry.as_str())
    } else {
        (resolved.as_path(), path)
    };
    app_paths
        .data_path_ref(entry)
        .or_else(|| app_paths.resource_path_ref(entry))
        .unwrap_or_else(|| fallback.to_owned())
}

fn save_skin_slot_history(skin: &mut SkinConfig, slot: SkinSlot, path: &str) {
    let options = skin_slot_options_mut(skin, slot).clone();
    let files = skin_slot_files_mut(skin, slot).clone();
    let offsets = skin_slot_offsets_mut(skin, slot).clone();
    skin.history.insert(
        skin_slot_history_key(slot, path),
        SkinHistoryEntryConfig { options, files, offsets },
    );
}

fn restore_skin_slot_history(skin: &mut SkinConfig, slot: SkinSlot, app_paths: &AppPaths) {
    let path = skin_slot_path(skin, slot).trim();
    let identity = skin_history_path(app_paths, skin, path);
    let history_key = skin_slot_history_key(slot, &identity);
    let prefix = skin_slot_history_key(slot, "");
    let entry = skin
        .history
        .get(&history_key)
        .or_else(|| skin.history.get(&skin_slot_history_key(slot, path)))
        .or_else(|| {
            skin.history.iter().find_map(|(key, entry)| {
                let stored_path = key.strip_prefix(&prefix)?;
                (skin_history_path(app_paths, skin, stored_path) == identity).then_some(entry)
            })
        })
        .or_else(|| skin.history.get(path))
        .or_else(|| {
            skin.history.iter().find_map(|(key, entry)| {
                (!key.contains("::") && skin_history_path(app_paths, skin, key) == identity)
                    .then_some(entry)
            })
        })
        .cloned();
    if let Some(entry) = &entry {
        skin.history.entry(history_key).or_insert_with(|| entry.clone());
    }
    let entry = entry.unwrap_or_default();

    *skin_slot_options_mut(skin, slot) = entry.options;
    *skin_slot_files_mut(skin, slot) = entry.files;
    *skin_slot_offsets_mut(skin, slot) = entry.offsets;
}
use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::SkinAccessGrant;

    #[test]
    fn dotted_roots_restore_options_files_and_offsets_from_legacy_paths() {
        let temp = tempfile::tempdir().unwrap();
        for data_dir in [temp.path().join("staging/../data"), PathBuf::from("./data")] {
            let paths = AppPaths::from_dirs(
                temp.path().join("staging/../resources"),
                data_dir,
                temp.path().join("cache"),
                temp.path().join("logs"),
            );
            for (old, selected) in [
                ("data/skins/B/play.lua", "data:skins/B/play.lua"),
                ("resource:skins/B/play.lua", "resource:skins/B/play.lua"),
            ] {
                let original = SkinHistoryEntryConfig {
                    options: BTreeMap::from([("Layout".into(), "Saved".into())]),
                    files: BTreeMap::from([("Notes".into(), "saved.png".into())]),
                    offsets: vec![SkinOffsetConfig { id: 30, x: 12, ..Default::default() }],
                };
                let mut skin = SkinConfig {
                    play7: old.into(),
                    play7_options: original.options.clone(),
                    play7_files: original.files.clone(),
                    play7_offsets: original.offsets.clone(),
                    ..Default::default()
                };
                assert!(switch_skin_slot(&mut skin, SkinSlot::Play7, &paths, String::new()));
                assert!(switch_skin_slot(&mut skin, SkinSlot::Play7, &paths, selected.into()));
                assert_eq!(skin.play7_options, original.options);
                assert_eq!(skin.play7_files, original.files);
                assert_eq!(skin.play7_offsets, original.offsets);
                assert!(
                    skin.history.contains_key(&skin_slot_history_key(SkinSlot::Play7, selected))
                );
            }
        }
    }

    fn app_paths(temp: &tempfile::TempDir) -> AppPaths {
        let root = temp.path().join("app");
        AppPaths::from_dirs(
            root.join("resources"),
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        )
    }

    #[test]
    fn authorized_external_skin_history_uses_the_pinned_entry() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_dirs(
            "resources".into(),
            std::path::absolute("data").unwrap(),
            "cache".into(),
            "logs".into(),
        );
        let slot = SkinSlot::Play7;
        let alias = "packs/../data/skins/Linked/play.luaskin";
        let user = "data:skins/A/play.luaskin";
        // The profile pins the external file even though the alias looks local after folding `..`.
        let external_root = temp.path().join("packs/data/skins/A");
        let entry = external_root.join("play.luaskin").to_string_lossy().into_owned();
        let grant = SkinAccessGrant {
            entry: entry.clone(),
            roots: vec![external_root.to_string_lossy().into_owned()],
        };
        let external_history = SkinHistoryEntryConfig {
            options: BTreeMap::from([("Layout".into(), "External".into())]),
            files: BTreeMap::from([("Notes".into(), "external.png".into())]),
            offsets: vec![SkinOffsetConfig { id: 30, x: 12, ..Default::default() }],
        };
        let mut skin = SkinConfig {
            external_access: BTreeMap::from([(
                paths.skin_access_key(Path::new(alias)).unwrap(),
                grant,
            )]),
            history: BTreeMap::from([(
                skin_slot_history_key(slot, alias),
                external_history.clone(),
            )]),
            ..Default::default()
        };

        assert!(switch_skin_slot(&mut skin, slot, &paths, user.into()));
        assert!(skin.play7_options.is_empty());
        assert!(skin.play7_files.is_empty());
        assert!(skin.play7_offsets.is_empty());
        skin.play7_options.insert("Layout".into(), "User".into());
        skin.play7_files.insert("Notes".into(), "user.png".into());
        skin.play7_offsets.push(SkinOffsetConfig { id: 30, x: -4, ..Default::default() });

        assert!(switch_skin_slot(&mut skin, slot, &paths, alias.into()));
        assert_eq!(skin.play7_options, external_history.options);
        assert_eq!(skin.play7_files, external_history.files);
        assert_eq!(skin.play7_offsets, external_history.offsets);
        assert_eq!(skin.history[&skin_slot_history_key(slot, &entry)], external_history);

        assert!(switch_skin_slot(&mut skin, slot, &paths, user.into()));
        assert_eq!(skin.play7_options["Layout"], "User");
        assert_eq!(skin.play7_files["Notes"], "user.png");
        assert_eq!(skin.play7_offsets[0].x, -4);
    }

    #[cfg(unix)]
    #[test]
    fn authorized_symlink_with_parent_dir_keeps_separate_skin_history() {
        use std::fs;
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let paths = app_paths(&temp);
        let app = paths.data_dir.parent().unwrap();
        let outside = temp.path().join("outside");
        let target = outside.join("current");
        let relative = "data/skins/A/play.luaskin";
        let external = outside.join(relative);
        for file in [&app.join(relative), &external] {
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, "return {}").unwrap();
        }
        fs::create_dir(&target).unwrap();
        symlink(target, app.join("packs")).unwrap();

        // Keep the alias relative without changing the test process's working directory.
        let cwd = std::env::current_dir().unwrap();
        let to_root: PathBuf = cwd.ancestors().skip(1).map(|_| "..").collect();
        let alias = to_root.join(app.strip_prefix("/").unwrap()).join("packs/..").join(relative);
        let alias_ref = alias.to_string_lossy().into_owned();
        let mut skin = SkinConfig {
            play7: alias_ref.clone(),
            play7_options: BTreeMap::from([("Layout".into(), "External".into())]),
            ..Default::default()
        };
        paths.approve_skin_path(&mut skin, &alias).unwrap();
        assert_eq!(
            paths.skin_path_context(&skin, &alias).unwrap().entry_file(),
            external.canonicalize().unwrap()
        );

        let slot = SkinSlot::Play7;
        let user = "data:skins/A/play.luaskin";
        assert!(switch_skin_slot(&mut skin, slot, &paths, user.into()));
        assert!(skin.play7_options.is_empty());
        skin.play7_options.insert("Layout".into(), "User".into());
        assert!(switch_skin_slot(&mut skin, slot, &paths, alias_ref));
        assert_eq!(skin.play7_options["Layout"], "External");
        assert!(switch_skin_slot(&mut skin, slot, &paths, user.into()));
        assert_eq!(skin.play7_options["Layout"], "User");
    }

    #[test]
    fn current_data_dir_keeps_bundled_and_user_skin_histories_separate() {
        let paths = AppPaths::from_dirs("..".into(), ".".into(), "cache".into(), "logs".into());
        let bundled = "resource:skins/A/play.luaskin";
        let user = "data:skins/A/play.luaskin";
        let mut skin = SkinConfig {
            play7: bundled.into(),
            play7_options: BTreeMap::from([("Layout".into(), "Bundled".into())]),
            play7_files: BTreeMap::from([("Notes".into(), "bundled.png".into())]),
            play7_offsets: vec![SkinOffsetConfig { id: 30, x: 12, ..Default::default() }],
            ..Default::default()
        };

        assert!(switch_skin_slot(&mut skin, SkinSlot::Play7, &paths, user.into()));
        assert!(skin.play7_options.is_empty());
        assert!(skin.play7_files.is_empty());
        assert!(skin.play7_offsets.is_empty());
        skin.play7_options.insert("Layout".into(), "User".into());
        skin.play7_files.insert("Notes".into(), "user.png".into());
        skin.play7_offsets.push(SkinOffsetConfig { id: 30, x: -4, ..Default::default() });

        for (path, layout, file, x) in
            [(bundled, "Bundled", "bundled.png", 12), (user, "User", "user.png", -4)]
        {
            assert!(switch_skin_slot(&mut skin, SkinSlot::Play7, &paths, path.into()));
            assert_eq!(skin.play7_options["Layout"], layout);
            assert_eq!(skin.play7_files["Notes"], file);
            assert_eq!(skin.play7_offsets[0].x, x);
            assert!(skin.history.contains_key(&skin_slot_history_key(SkinSlot::Play7, path)));
        }
    }

    #[test]
    fn parent_dir_skin_path_does_not_share_history_with_a_real_subdirectory() {
        let temp = tempfile::tempdir().unwrap();
        let paths = app_paths(&temp);
        // 手書き設定の絶対パス。実際には data/skins/B を指す。
        let dotted = paths.data_dir.join("skins/A/../B/play.lua").to_string_lossy().into_owned();
        let mut skin = SkinConfig {
            play7: dotted,
            play7_options: BTreeMap::from([("marker".to_owned(), "B".to_owned())]),
            ..Default::default()
        };

        // A/B は実在する別スキンなので、B の履歴を引き継いではいけない。
        assert!(switch_skin_slot(
            &mut skin,
            SkinSlot::Play7,
            &paths,
            "data:skins/A/B/play.lua".into()
        ));
        assert_eq!(skin.play7, "data:skins/A/B/play.lua");
        assert!(!skin.play7_options.contains_key("marker"));

        // B に戻すと元の履歴が復元される。
        assert!(switch_skin_slot(
            &mut skin,
            SkinSlot::Play7,
            &paths,
            "data:skins/B/play.lua".into()
        ));
        assert_eq!(skin.play7_options.get("marker").map(String::as_str), Some("B"));

        // 履歴キーも B と A/B で分かれている。
        assert!(skin.history.contains_key("play7::data:skins/B/play.lua"));
        assert!(skin.history.contains_key("play7::data:skins/A/B/play.lua"));
        assert_ne!(
            skin.history["play7::data:skins/B/play.lua"].options,
            skin.history["play7::data:skins/A/B/play.lua"].options
        );
    }
}
