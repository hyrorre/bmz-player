use super::*;

const RANDOM_MIX_COURSE_KEY: &str = "random-mix";

impl WinitApp {
    pub(super) fn start_random_mix(&mut self) {
        let mut charts = match self.boot.library_db.list_all_charts() {
            Ok(charts) => charts,
            Err(error) => {
                tracing::error!(%error, "failed to load RANDOM MIX candidates");
                self.show_left_overlay_toast(
                    Localizer::new(self.boot.profile_config.ui.locale())
                        .text("toast-select-random-mix-failed"),
                );
                return;
            }
        };
        let active_song_roots = enabled_root_paths(&self.boot.app_config);
        charts.retain(|chart| chart_is_in_active_song_roots(chart, Some(&active_song_roots)));
        let key_mode = self.selected_source_key_mode().unwrap_or(KeyMode::K7);
        let seed = crate::random_option_seed::fresh_bms_random_seed();
        let config = self.boot.profile_config.select.random_mix;
        let Some(definition) = build_random_mix_definition(&charts, config, key_mode, seed) else {
            self.show_left_overlay_toast(
                Localizer::new(self.boot.profile_config.ui.locale())
                    .text("toast-select-random-mix-empty"),
            );
            tracing::warn!(mode = key_mode.as_str(), "no chart matched RANDOM MIX constraints");
            return;
        };
        let entries = definition.entries.len();
        let course_id = match self.boot.library_db.upsert_course(
            RANDOM_MIX_COURSE_SOURCE,
            &definition,
            0,
            now_unix_seconds(),
        ) {
            Ok(course_id) => course_id,
            Err(error) => {
                tracing::error!(%error, "failed to store RANDOM MIX course");
                self.show_left_overlay_toast(
                    Localizer::new(self.boot.profile_config.ui.locale())
                        .text("toast-select-random-mix-failed"),
                );
                return;
            }
        };
        tracing::info!(course_id, entries, seed, mode = key_mode.as_str(), "created RANDOM MIX");
        self.start_course(course_id);
    }

    pub(super) fn adjust_random_mix_skin_option(&mut self, event_id: i32, arg: i32) {
        let Some(entry_id) = random_mix_settings_entry(event_id) else {
            return;
        };
        let direction = if arg >= 0 { 1 } else { -1 };
        let delta = direction * crate::config::settings_registry::settings_adjust_step(entry_id);
        if crate::config::settings_registry::adjust_settings_value(
            &mut self.boot.profile_config,
            entry_id,
            delta,
        ) {
            self.boot.profile_config.updated_at = now_unix_seconds();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        }
    }
}

fn random_mix_settings_entry(event_id: i32) -> Option<SettingsEntryId> {
    Some(match event_id {
        260 => SettingsEntryId::RandomMixTargetLevel,
        261 => SettingsEntryId::RandomMixMaxLevel,
        262 => SettingsEntryId::RandomMixMinLevel,
        263 => SettingsEntryId::RandomMixBpmRange,
        264 => SettingsEntryId::RandomMixMaxBpm,
        265 => SettingsEntryId::RandomMixMinBpm,
        266 => SettingsEntryId::RandomMixStages,
        _ => return None,
    })
}

pub(super) fn build_random_mix_definition(
    charts: &[ChartListItem],
    config: crate::config::profile_config::RandomMixConfig,
    key_mode: KeyMode,
    seed: u64,
) -> Option<bmz_core::course::CourseDefinition> {
    let mut rng = RandomMixRng::new(seed);
    let stages =
        if (1..=5).contains(&config.stages) { config.stages as usize } else { 2 + rng.index(3) };
    let (min_level, max_level) = normalized_optional_bounds(config.min_level, config.max_level);
    let (min_bpm, max_bpm) = normalized_optional_bounds(config.min_bpm, config.max_bpm);
    let mut first_candidates = charts
        .iter()
        .filter(|chart| {
            random_mix_candidate_matches(chart, key_mode, min_level, max_level, min_bpm, max_bpm)
                && (config.bpm_range == 0 || (chart.max_bpm - chart.min_bpm).abs() < f64::EPSILON)
        })
        .collect::<Vec<_>>();
    rng.shuffle(&mut first_candidates);
    let first_candidate = first_candidates.into_iter().next()?;
    let first = random_mix_target_chart(
        charts,
        first_candidate,
        key_mode,
        min_level,
        max_level,
        min_bpm,
        max_bpm,
        None,
        config.bpm_range,
        config.target_level,
    );
    let reference_bpm = first.max_bpm;
    let mut selected = Vec::with_capacity(stages);
    selected.push(first);

    let mut remaining_candidates = charts
        .iter()
        .filter(|chart| {
            random_mix_candidate_matches(chart, key_mode, min_level, max_level, min_bpm, max_bpm)
                && (config.bpm_range == 0
                    || (chart.min_bpm >= reference_bpm - config.bpm_range as f64
                        && chart.max_bpm <= reference_bpm + config.bpm_range as f64))
        })
        .collect::<Vec<_>>();
    rng.shuffle(&mut remaining_candidates);
    let mut selected_folders = HashSet::from([first.folder_path.as_str()]);
    for candidate in remaining_candidates {
        if selected.len() >= stages {
            break;
        }
        if !selected_folders.insert(candidate.folder_path.as_str()) {
            continue;
        }
        selected.push(random_mix_target_chart(
            charts,
            candidate,
            key_mode,
            min_level,
            max_level,
            min_bpm,
            max_bpm,
            Some(reference_bpm),
            config.bpm_range,
            config.target_level,
        ));
    }

    Some(bmz_core::course::CourseDefinition {
        key: RANDOM_MIX_COURSE_KEY.to_string(),
        title: "RANDOM MIX".to_string(),
        kind: bmz_core::course::CourseKind::Course,
        entries: selected
            .into_iter()
            .map(|chart| bmz_core::course::CourseEntry {
                title_hint: chart.title.clone(),
                md5: Some(hash_to_hex(&chart.md5)),
                sha256: Some(hash_to_hex(&chart.sha256)),
                chart_id: Some(chart.chart_id),
            })
            .collect(),
        constraints: bmz_core::course::CourseConstraints::default(),
        trophies: Vec::new(),
        // RANDOM MIX is an ephemeral local course. Never submit its generated definition or
        // result to IR, even when the current profile has an enabled provider.
        release: false,
    })
}

fn normalized_optional_bounds(min: u32, max: u32) -> (u32, u32) {
    if min > 0 && max > 0 && min > max { (max, min) } else { (min, max) }
}

fn chart_level(chart: &ChartListItem) -> u32 {
    chart.play_level.trim().parse().unwrap_or_default()
}

fn random_mix_level_matches(chart: &ChartListItem, min: u32, max: u32) -> bool {
    let level = chart_level(chart);
    (min == 0 || level >= min) && (max == 0 || level <= max)
}

fn random_mix_bpm_matches(chart: &ChartListItem, min: u32, max: u32) -> bool {
    chart.min_bpm.is_finite()
        && chart.max_bpm.is_finite()
        && (min == 0 || chart.min_bpm >= min as f64)
        && (max == 0 || chart.max_bpm <= max as f64)
}

fn random_mix_candidate_matches(
    chart: &ChartListItem,
    key_mode: KeyMode,
    min_level: u32,
    max_level: u32,
    min_bpm: u32,
    max_bpm: u32,
) -> bool {
    chart.mode == key_mode.as_str()
        && random_mix_level_matches(chart, min_level, max_level)
        && random_mix_bpm_matches(chart, min_bpm, max_bpm)
}

fn random_mix_target_chart<'a>(
    charts: &'a [ChartListItem],
    default: &'a ChartListItem,
    key_mode: KeyMode,
    min_level: u32,
    max_level: u32,
    min_bpm: u32,
    max_bpm: u32,
    reference_bpm: Option<f64>,
    bpm_range: u32,
    target_level: u32,
) -> &'a ChartListItem {
    if target_level == 0 {
        return default;
    }
    charts
        .iter()
        .filter(|chart| {
            chart.folder_path == default.folder_path
                && chart.mode == key_mode.as_str()
                && random_mix_level_matches(chart, min_level, max_level)
                && random_mix_bpm_matches(chart, min_bpm, max_bpm)
                && match reference_bpm {
                    Some(reference_bpm) => {
                        bpm_range == 0
                            || (chart.min_bpm >= reference_bpm - bpm_range as f64
                                && chart.max_bpm <= reference_bpm + bpm_range as f64)
                    }
                    None => bpm_range == 0 || (chart.max_bpm - chart.min_bpm).abs() < f64::EPSILON,
                }
        })
        .min_by_key(|chart| chart_level(chart).abs_diff(target_level))
        .unwrap_or(default)
}

struct RandomMixRng(u64);

impl RandomMixRng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0x9e37_79b9_7f4a_7c15 } else { seed })
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        value.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn index(&mut self, len: usize) -> usize {
        (self.next() % len as u64) as usize
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            values.swap(index, self.index(index + 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(
        id: i64,
        folder: &str,
        mode: &str,
        level: &str,
        min_bpm: f64,
        max_bpm: f64,
    ) -> ChartListItem {
        ChartListItem {
            chart_id: id,
            md5: [id as u8; 16],
            sha256: [id as u8; 32],
            title: format!("Chart {id}"),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: level.to_string(),
            mode: mode.to_string(),
            total_notes: 100,
            initial_bpm: min_bpm,
            min_bpm,
            max_bpm,
            length_ms: 60_000,
            folder_path: folder.to_string(),
            stage_file: String::new(),
            banner_file: String::new(),
            backbmp_file: String::new(),
            preview_file: String::new(),
            has_document: false,
            has_bga: false,
            has_long_notes: false,
            has_mines: false,
            has_bms_random: false,
            judge_rank: None,
            bms_total: 0.0,
            ln_profile: Default::default(),
            ln_counts: Default::default(),
        }
    }

    #[test]
    fn random_mix_filters_mode_and_level_and_uses_distinct_song_folders() {
        let charts = vec![
            chart(1, "a", "7K", "4", 120.0, 120.0),
            chart(2, "a", "7K", "8", 120.0, 120.0),
            chart(3, "b", "7K", "9", 140.0, 140.0),
            chart(4, "c", "14K", "9", 140.0, 140.0),
            chart(5, "d", "7K", "12", 140.0, 140.0),
        ];
        let config = crate::config::profile_config::RandomMixConfig {
            min_level: 5,
            max_level: 10,
            bpm_range: 0,
            stages: 5,
            ..Default::default()
        };

        let definition = build_random_mix_definition(&charts, config, KeyMode::K7, 7).unwrap();

        assert_eq!(definition.entries.len(), 2);
        let ids =
            definition.entries.iter().map(|entry| entry.chart_id.unwrap()).collect::<HashSet<_>>();
        assert_eq!(ids, HashSet::from([2, 3]));
    }

    #[test]
    fn random_mix_target_level_and_bpm_range_follow_lr2_rules() {
        let charts = vec![
            chart(1, "a", "7K", "3", 150.0, 150.0),
            chart(2, "a", "7K", "10", 150.0, 150.0),
            chart(3, "b", "7K", "9", 155.0, 155.0),
            chart(4, "c", "7K", "9", 180.0, 180.0),
        ];
        let config = crate::config::profile_config::RandomMixConfig {
            target_level: 9,
            bpm_range: 10,
            stages: 5,
            ..Default::default()
        };

        let definition = build_random_mix_definition(&charts, config, KeyMode::K7, 11).unwrap();
        let ids =
            definition.entries.iter().map(|entry| entry.chart_id.unwrap()).collect::<HashSet<_>>();

        assert!(ids.contains(&2));
        assert_eq!(ids.len(), 2);
        assert!(!ids.contains(&4));
    }

    #[test]
    fn random_mix_zero_stage_count_selects_two_to_four_stages() {
        let charts = (1..=5)
            .map(|id| chart(id, &format!("folder-{id}"), "7K", "7", 120.0, 120.0))
            .collect::<Vec<_>>();
        let config = crate::config::profile_config::RandomMixConfig {
            bpm_range: 0,
            stages: 0,
            ..Default::default()
        };

        let definition = build_random_mix_definition(&charts, config, KeyMode::K7, 3).unwrap();

        assert!((2..=4).contains(&definition.entries.len()));
    }

    #[test]
    fn random_mix_is_never_released_to_ir() {
        let charts = vec![chart(1, "a", "7K", "7", 120.0, 120.0)];

        let definition = build_random_mix_definition(
            &charts,
            crate::config::profile_config::RandomMixConfig::default(),
            KeyMode::K7,
            3,
        )
        .unwrap();

        assert!(!definition.release);
    }

    #[test]
    fn random_mix_target_level_does_not_bypass_bpm_limits() {
        let charts = vec![
            chart(1, "a", "7K", "3", 120.0, 120.0),
            chart(2, "a", "7K", "10", 200.0, 200.0),
            chart(3, "b", "7K", "9", 125.0, 125.0),
        ];
        let config = crate::config::profile_config::RandomMixConfig {
            target_level: 10,
            max_bpm: 130,
            stages: 2,
            ..Default::default()
        };

        let definition = build_random_mix_definition(&charts, config, KeyMode::K7, 7).unwrap();
        let ids =
            definition.entries.iter().map(|entry| entry.chart_id.unwrap()).collect::<HashSet<_>>();

        assert!(ids.contains(&1));
        assert!(!ids.contains(&2));
    }

    #[test]
    fn random_mix_skin_events_map_to_lr2_settings() {
        let expected = [
            SettingsEntryId::RandomMixTargetLevel,
            SettingsEntryId::RandomMixMaxLevel,
            SettingsEntryId::RandomMixMinLevel,
            SettingsEntryId::RandomMixBpmRange,
            SettingsEntryId::RandomMixMaxBpm,
            SettingsEntryId::RandomMixMinBpm,
            SettingsEntryId::RandomMixStages,
        ];
        for (offset, entry_id) in expected.into_iter().enumerate() {
            assert_eq!(random_mix_settings_entry(260 + offset as i32), Some(entry_id));
        }
        assert_eq!(random_mix_settings_entry(259), None);
        assert_eq!(random_mix_settings_entry(267), None);
    }
}
