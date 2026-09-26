pub(in crate::app) fn enabled_root_paths(
    app_config: &crate::config::app_config::AppConfig,
) -> Vec<String> {
    app_config.songs.roots.iter().filter(|p| p.enabled).map(|p| p.path.clone()).collect()
}

pub(in crate::app) fn table_source_order(
    app_config: &crate::config::app_config::AppConfig,
) -> Vec<String> {
    app_config
        .tables
        .sources
        .iter()
        .filter(|source| source.enabled)
        .map(|source| source.url.clone())
        .collect()
}

/// 選曲リストを構築し、mode filter / sort を適用して返す。
///
/// mode filter は beatoraja `BarManager` 準拠で、指定モードがこの一覧の
/// チャートを「全て」消してしまう場合のみ、チャートが残るモードへ前方向に
/// 自動送りする。実際に適用したモードを items と共に返すので、呼び出し側で
/// 永続化 / 表示状態を更新できる。
pub(in crate::app) fn load_items_for_stack(
    boot: &crate::bootstrap::BootstrappedApp,
    collection_cache: &mut crate::screens::select_model::SelectCollectionCache,
    stack: &[String],
    search_history: &[String],
    mode_filter: SelectModeFilter,
    difficulty_filter: SelectDifficultyFilter,
    sort: SelectSort,
) -> (Vec<SelectItem>, SelectModeFilter) {
    let mut items = build_select_items_for_stack(boot, stack, search_history);
    let ordered_course_contents = stack.last().and_then(|path| parse_course_contents_path(path));
    if ordered_course_contents.is_some() {
        if let Err(error) =
            collection_cache.apply(&boot.library_db, &boot.collection_db, &mut items)
        {
            tracing::error!(%error, "failed to apply collection flags to course contents");
        }
        return (items, mode_filter);
    }
    let resolved = resolve_non_empty_mode_filter(&items, mode_filter);
    apply_select_mode_filter(&mut items, resolved);
    apply_select_difficulty_filter(&mut items, difficulty_filter);
    apply_select_sort(&mut items, sort);
    if let Err(error) = collection_cache.apply(&boot.library_db, &boot.collection_db, &mut items) {
        tracing::error!(%error, "failed to apply collection flags to select items");
    }
    let random_items = random_select_items_from_items(&items, &boot.profile_config.select);
    items.splice(0..0, random_items);
    (items, resolved)
}

pub(in crate::app) fn build_select_items_for_stack(
    boot: &crate::bootstrap::BootstrappedApp,
    stack: &[String],
    search_history: &[String],
) -> Vec<SelectItem> {
    let active_song_roots = enabled_root_paths(&boot.app_config);
    let mut active_table_sources = table_source_order(&boot.app_config);
    if let Some(identity) = RianTableIdentity::from_ir_config(&boot.profile_config.ir) {
        match active_rian_table_source_urls(&boot.library_db, &identity) {
            Ok(sources) => active_table_sources.extend(sources),
            Err(error) => tracing::warn!(%error, "failed to load cached rianIR table sources"),
        }
    }
    match stack.last() {
        Some(path) if path.starts_with(crate::screens::settings_model::CONFIG_ROOT_PATH) => {
            crate::screens::settings_model::load_settings_items_for_config(
                path,
                boot.profile_config.ui.locale(),
                &boot.app_config,
                &boot.profile_config,
            )
        }
        Some(path) if path == COURSE_ROOT_PATH => {
            let mut items = match load_select_items_for_courses(
                &boot.library_db,
                &boot.score_db,
                boot.profile_config.play.ln_mode_policy,
                boot.profile_config.play.rule_mode,
            ) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load course list");
                    Vec::new()
                }
            };
            items.push(new_course_item_for_locale(boot.profile_config.ui.locale()));
            items.push(random_mix_item());
            items
        }
        Some(path) if parse_course_contents_path(path).is_some() => {
            let course_id = parse_course_contents_path(path).unwrap();
            match load_select_items_for_course_contents(
                &boot.library_db,
                &boot.score_db,
                course_id,
                boot.profile_config.play.ln_mode_policy,
                boot.profile_config.play.rule_mode,
            ) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, course_id, "failed to load course contents");
                    Vec::new()
                }
            }
        }
        Some(path) if path == FAVORITE_ROOT_PATH => {
            match favorite_root_items(&boot.collection_db) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load favorite root items");
                    Vec::new()
                }
            }
        }
        Some(path) if path == FAVORITE_CHART_PATH => {
            match load_select_items_for_favorite_charts(
                &boot.library_db,
                &boot.score_db,
                &boot.collection_db,
                boot.profile_config.play.ln_mode_policy,
                boot.profile_config.play.rule_mode,
                &active_table_sources,
                Some(&active_song_roots),
                Some(&active_table_sources),
            ) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load favorite chart items");
                    Vec::new()
                }
            }
        }
        Some(path) if path == FAVORITE_SONG_PATH => {
            match load_select_items_for_favorite_songs(&boot.collection_db) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load favorite song folders");
                    Vec::new()
                }
            }
        }
        Some(path) if parse_favorite_song_detail_path(path).is_some() => {
            let representative_sha256 = parse_favorite_song_detail_path(path).unwrap();
            match load_select_items_for_favorite_song(
                &boot.library_db,
                &boot.score_db,
                &boot.collection_db,
                representative_sha256,
                boot.profile_config.play.ln_mode_policy,
                boot.profile_config.play.rule_mode,
                &active_table_sources,
                Some(&active_song_roots),
                Some(&active_table_sources),
            ) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load favorite song items");
                    Vec::new()
                }
            }
        }
        Some(path) if path.starts_with(SEARCH_PATH_PREFIX) => match parse_search_query(path) {
            Some(query) => {
                match load_select_items_for_search_for_rule_mode_with_filters(
                    &boot.library_db,
                    &boot.score_db,
                    query,
                    boot.profile_config.play.ln_mode_policy,
                    boot.profile_config.play.rule_mode,
                    &active_table_sources,
                    Some(&active_song_roots),
                    Some(&active_table_sources),
                ) {
                    Ok(items) => items,
                    Err(error) => {
                        tracing::error!(%error, query, "failed to load search results");
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        },
        Some(path) if parse_same_folder_path(path).is_some() => {
            let folder = parse_same_folder_path(path).unwrap();
            match load_select_items_in_folder_for_rule_mode_with_filters(
                &boot.library_db,
                &boot.score_db,
                folder,
                boot.profile_config.play.ln_mode_policy,
                boot.profile_config.play.rule_mode,
                &active_table_sources,
                Some(&active_song_roots),
                Some(&active_table_sources),
            ) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load same-folder items");
                    Vec::new()
                }
            }
        }
        Some(path) if path.starts_with(VIRTUAL_FOLDER_PATH_PREFIX) => {
            match load_select_items_in_virtual_folder(
                &boot.library_db,
                &boot.score_db,
                &boot.profile_paths.root_dir,
                path,
                boot.profile_config.play.ln_mode_policy,
                boot.profile_config.play.rule_mode,
                Some(&active_song_roots),
            ) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(
                        %error,
                        path,
                        "failed to load virtual-folder items"
                    );
                    Vec::new()
                }
            }
        }
        Some(path) if path.starts_with(TABLE_ROOT_PATH) => match parse_table_path(path) {
            Some(TablePath::Root) => {
                match table_folder_items_for_active_sources(
                    &boot.library_db,
                    &active_table_sources,
                    Some(&active_table_sources),
                ) {
                    Ok(items) => items,
                    Err(error) => {
                        tracing::error!(%error, "failed to load difficulty table list");
                        Vec::new()
                    }
                }
            }
            Some(TablePath::Table { source_url }) => {
                if !active_table_sources.iter().any(|url| url == source_url) {
                    return Vec::new();
                }
                match table_level_folder_items(
                    &boot.library_db,
                    &boot.score_db,
                    source_url,
                    boot.profile_config.play.ln_mode_policy,
                    boot.profile_config.play.rule_mode,
                ) {
                    Ok(items) => items,
                    Err(error) => {
                        tracing::error!(%error, "failed to load difficulty table levels");
                        Vec::new()
                    }
                }
            }
            Some(TablePath::Level { source_url, level }) => {
                if !active_table_sources.iter().any(|url| url == source_url) {
                    return Vec::new();
                }
                match load_select_items_in_table_level_for_rule_mode(
                    &boot.library_db,
                    &boot.score_db,
                    source_url,
                    level,
                    boot.profile_config.play.ln_mode_policy,
                    boot.profile_config.play.rule_mode,
                ) {
                    Ok(items) => items,
                    Err(error) => {
                        tracing::error!(%error, "failed to load difficulty table charts");
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        },
        Some(folder) => {
            match load_select_items_in_folder_for_rule_mode_with_filters(
                &boot.library_db,
                &boot.score_db,
                folder,
                boot.profile_config.play.ln_mode_policy,
                boot.profile_config.play.rule_mode,
                &active_table_sources,
                Some(&active_song_roots),
                Some(&active_table_sources),
            ) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load select items");
                    Vec::new()
                }
            }
        }
        None => {
            // ルートには曲フォルダに続けて、コースフォルダ・各難易度表フォルダを並べる。
            // COURSE は選曲画面からの新規作成入口も兼ねるため、保存済みコースがなくても表示する。
            let mut items = root_folder_items(&active_song_roots);
            match favorite_root_items(&boot.collection_db) {
                Ok(favorites) if !favorites.is_empty() => items.push(favorite_root_item()),
                Ok(_) => {}
                Err(error) => tracing::error!(%error, "failed to check favorite root"),
            }
            items.push(course_root_item());
            match table_folder_items_for_active_sources(
                &boot.library_db,
                &active_table_sources,
                Some(&active_table_sources),
            ) {
                Ok(tables) => items.extend(tables),
                Err(error) => {
                    tracing::error!(%error, "failed to load difficulty table folders");
                }
            }
            match virtual_folder_root_items(&boot.profile_paths.root_dir) {
                Ok(folders) => items.extend(folders),
                Err(error) => {
                    tracing::error!(%error, "failed to load virtual-folder catalog");
                }
            }
            items.push(settings_root_item_for_locale(boot.profile_config.ui.locale()));
            if !search_history.is_empty() {
                items.extend(search_history_folder_items_for_locale(
                    search_history,
                    boot.profile_config.ui.locale(),
                ));
            }
            items
        }
    }
}
use super::*;
