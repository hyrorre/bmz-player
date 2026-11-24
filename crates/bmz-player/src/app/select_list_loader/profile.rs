use super::*;

#[test]
#[ignore = "release probe with an explicit local benchmark data directory"]
fn profile_select_navigation() -> Result<()> {
    use crate::screens::select_model::{
        select_folder_summary_for_rule_mode, virtual_folder_summary,
    };
    use crate::storage::{library_db::LibraryDatabase, score_db::ScoreDatabase};
    use bmz_render::scene::SelectRowKind;
    use std::hash::{DefaultHasher, Hash, Hasher};
    let folder = std::env::var("BMZ_SELECT_BENCH_FOLDER")?;
    let large = std::env::var("BMZ_SELECT_BENCH_LARGE_FOLDER")?;
    anyhow::ensure!(
        std::env::var_os("BMZ_DATA_DIR").is_some(),
        "set BMZ_DATA_DIR to a benchmark fixture"
    );
    let paths = crate::paths::resolve_app_paths()?;
    let profile = crate::paths::resolve_profile_paths(&paths, "bench")?;
    let app_config = toml::from_str(&std::fs::read_to_string(&paths.config_toml)?)?;
    let profile_config: ProfileConfig =
        toml::from_str(&std::fs::read_to_string(&profile.profile_toml)?)?;
    let roots = enabled_root_paths(&app_config);
    let library = LibraryDatabase::open_read_only(&paths.library_db)?;
    let scores = ScoreDatabase::open_read_only(&profile.score_db)?;
    let ln = profile_config.play.ln_mode_policy;
    let rule = profile_config.play.rule_mode;
    let mut request = SelectListRequest {
        action: SelectListAction::Refresh,
        app_config,
        profile_config,
        library_path: paths.library_db,
        profile_paths: profile.clone(),
        folder_stack: vec![],
        selected_index_stack: vec![],
        selected_index: 0,
        selected_key: None,
        history: vec![],
        mode: SelectModeFilter::All,
        difficulty: SelectDifficultyFilter::All,
        sort: SelectSort::Title,
    };
    let folder = folder.as_str();
    for (label, action, stack) in [
        ("enter", SelectListAction::Enter(folder.into()), vec![]),
        ("exit", SelectListAction::Exit, vec![folder.into()]),
        ("large", SelectListAction::Enter(large.clone()), vec![]),
        ("virtual", SelectListAction::Enter("bmz-filter:level/7k/level-12".into()), vec![]),
    ] {
        request.action = action;
        request.folder_stack = stack;
        let now = Instant::now();
        let list = load(&request)?.prepared.unwrap();
        let elapsed = now.elapsed();
        let mut digest = DefaultHasher::new();
        for item in &list.items {
            format!("{item:?}").hash(&mut digest);
        }
        println!("PROBE {label} {} {} {}", elapsed.as_micros(), list.items.len(), digest.finish());
    }
    for path in [
        folder,
        large.as_str(),
        "bmz-filter:level/7k/level-12",
        "bmz-filter:my-best",
        "bmz-filter:lamp-update/day-0",
    ] {
        let now = Instant::now();
        let summary = if path.starts_with("bmz-filter:") {
            virtual_folder_summary(&library, &scores, &profile.root_dir, path, ln, rule, &roots)?
        } else {
            select_folder_summary_for_rule_mode(
                &library,
                &scores,
                path,
                SelectRowKind::Folder,
                ln,
                rule,
            )?
            .unwrap()
        };
        println!("PROBE {path} {} {summary:?}", now.elapsed().as_micros());
    }
    let mut items: Vec<_> = (0..10000)
        .map(|i| SelectItem::Folder {
            path: format!("folder/{i}"),
            name: i.to_string(),
            kind: SelectRowKind::Folder,
            summary: None,
        })
        .collect();
    let indices = select_visible_item_indices(items.len(), 0, 25);
    let mut runtime =
        super::super::select_folder_summary::SelectFolderSummaryRuntime::disabled(&[], ln, rule);
    runtime.refresh(&mut items, &indices, ln, rule, &roots, None);
    let now = Instant::now();
    for _ in 0..300 {
        runtime.refresh(std::hint::black_box(&mut items), &indices, ln, rule, &roots, None);
    }
    println!("PROBE poll {}", now.elapsed().as_micros() / 300);
    Ok(())
}
