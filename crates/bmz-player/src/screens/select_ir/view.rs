use super::*;

impl SelectIrRanking {
    /// 選択中譜面のスキン用 snapshot。IR 未設定なら Offline。
    pub fn snapshot_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
    ) -> ResultIrSnapshot {
        self.snapshot_for_scope(ir_config, selected, SelectIrRankingScope::Global)
    }

    /// 対応 Select スキン用の、現在選択されている scope の snapshot。
    pub fn active_snapshot_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
    ) -> ResultIrSnapshot {
        self.snapshot_for_scope(ir_config, selected, self.active_scope)
    }

    /// Select スキンの scope binding に従う snapshot を返す。
    ///
    /// `Global` は既存スキンとの互換性を保ち、常に全体ランキングを返す。
    pub fn snapshot_for_binding(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
        binding: bmz_render::skin::IrScopeBinding,
    ) -> ResultIrSnapshot {
        match binding {
            bmz_render::skin::IrScopeBinding::Global => self.snapshot_for(ir_config, selected),
            bmz_render::skin::IrScopeBinding::Active => {
                self.active_snapshot_for(ir_config, selected)
            }
        }
    }

    /// 指定 scope の snapshot。Rival scope が未取得・非対応なら Global へ戻す。
    pub fn snapshot_for_scope(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
        requested_scope: SelectIrRankingScope,
    ) -> ResultIrSnapshot {
        if enabled_provider(ir_config).is_none() {
            return ResultIrSnapshot::default();
        }
        let Some(sha256) = selected else {
            return snapshot_with_provider(ResultIrSnapshot::default(), ir_config);
        };
        let scope = if requested_scope == SelectIrRankingScope::SelfAndRivals
            && self.supports_scope(ir_config, Some(sha256), requested_scope)
        {
            requested_scope
        } else {
            SelectIrRankingScope::Global
        };
        let mut snapshot = self
            .cache
            .get(&sha256)
            .and_then(|entry| {
                let mut snapshot = match scope {
                    SelectIrRankingScope::Global => entry
                        .global_ranking
                        .as_ref()
                        .map(|ranking| {
                            result_ir_ranking_to_skin_snapshot_at(
                                ranking,
                                entry.global_scroll_offset,
                            )
                        })
                        .unwrap_or_else(|| entry.global.clone()),
                    SelectIrRankingScope::SelfAndRivals => entry
                        .self_and_rivals_ranking
                        .as_ref()
                        .map(|ranking| {
                            result_ir_ranking_to_skin_snapshot_at(
                                ranking,
                                entry.self_and_rivals_scroll_offset,
                            )
                        })
                        .or_else(|| entry.self_and_rivals.clone())?,
                };
                let entry = &self.cache[&sha256];
                match snapshot.state {
                    SkinIrState::Loaded => {
                        snapshot.connect_success_ms = Some(elapsed_since_ms(entry.completed_at));
                    }
                    SkinIrState::Failed => {
                        snapshot.connect_fail_ms = Some(elapsed_since_ms(entry.completed_at));
                    }
                    _ => {}
                }
                Some(snapshot)
            })
            .unwrap_or_else(|| {
                let begin_ms = self
                    .in_flight
                    .as_ref()
                    .filter(|(_, in_flight_sha, _)| *in_flight_sha == sha256)
                    .map(|(_, _, started_at)| elapsed_since_ms(*started_at));
                ResultIrSnapshot {
                    state: if begin_ms.is_some() {
                        SkinIrState::Loading
                    } else {
                        SkinIrState::Waiting
                    },
                    connect_begin_ms: begin_ms,
                    ..Default::default()
                }
            });
        snapshot.scope = match scope {
            SelectIrRankingScope::Global => SkinIrScope::Global,
            SelectIrRankingScope::SelfAndRivals => SkinIrScope::Rival,
        };
        snapshot.global_scope_supported = true;
        snapshot.rival_scope_supported =
            self.supports_scope(ir_config, Some(sha256), SelectIrRankingScope::SelfAndRivals);
        snapshot_with_provider(snapshot, ir_config)
    }

    pub fn active_scope(&self) -> SelectIrRankingScope {
        self.active_scope
    }

    pub fn battle_entries_for(&self, selected: Option<[u8; 32]>) -> &[SelectIrBattleEntry] {
        let Some(entry) = selected.and_then(|sha256| self.cache.get(&sha256)) else {
            return &[];
        };
        match self.active_scope {
            SelectIrRankingScope::Global => &entry.global_battle_entries,
            SelectIrRankingScope::SelfAndRivals => &entry.self_and_rivals_battle_entries,
        }
    }

    /// scope を選ぶ。Self-and-Rivals は取得済みの場合だけ選択できる。
    pub fn select_scope(
        &mut self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
        scope: SelectIrRankingScope,
    ) -> bool {
        if self.active_scope == scope || !self.supports_scope(ir_config, selected, scope) {
            return false;
        }
        self.active_scope = scope;
        true
    }

    pub fn toggle_scope(&mut self, ir_config: &IrConfig, selected: Option<[u8; 32]>) -> bool {
        let next = match self.active_scope {
            SelectIrRankingScope::Global => SelectIrRankingScope::SelfAndRivals,
            SelectIrRankingScope::SelfAndRivals => SelectIrRankingScope::Global,
        };
        self.select_scope(ir_config, selected, next)
    }

    /// コース行は global のみ。単曲の Self-and-Rivals は取得済み時だけ有効にする。
    pub fn supports_scope(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
        scope: SelectIrRankingScope,
    ) -> bool {
        enabled_provider(ir_config).is_some()
            && match scope {
                SelectIrRankingScope::Global => selected.is_some(),
                SelectIrRankingScope::SelfAndRivals => selected
                    .and_then(|sha256| self.cache.get(&sha256))
                    .and_then(|entry| entry.self_and_rivals.as_ref())
                    .is_some(),
            }
    }

    /// Select skin の changeable slider type 8 からランキングの表示位置を更新する。
    pub fn set_skin_scroll_rate(
        &mut self,
        selected_chart: Option<[u8; 32]>,
        selected_course: Option<&SelectCourseIrTarget>,
        binding: bmz_render::skin::IrScopeBinding,
        value: f32,
    ) {
        if let Some(target) = selected_course {
            let Some(entry) = self.course_cache.get_mut(target) else {
                return;
            };
            let Some(ranking) = entry.ranking.as_ref() else {
                return;
            };
            entry.scroll_offset = skin_scroll_offset_from_rate(value, ranking.entries.len());
            return;
        }

        let Some(sha256) = selected_chart else {
            return;
        };
        let Some(entry) = self.cache.get_mut(&sha256) else {
            return;
        };
        let scope = match binding {
            bmz_render::skin::IrScopeBinding::Global => SelectIrRankingScope::Global,
            bmz_render::skin::IrScopeBinding::Active
                if self.active_scope == SelectIrRankingScope::SelfAndRivals
                    && entry.self_and_rivals_ranking.is_some() =>
            {
                SelectIrRankingScope::SelfAndRivals
            }
            bmz_render::skin::IrScopeBinding::Active => SelectIrRankingScope::Global,
        };
        match scope {
            SelectIrRankingScope::Global => {
                if let Some(ranking) = entry.global_ranking.as_ref() {
                    entry.global_scroll_offset =
                        skin_scroll_offset_from_rate(value, ranking.entries.len());
                }
            }
            SelectIrRankingScope::SelfAndRivals => {
                if let Some(ranking) = entry.self_and_rivals_ranking.as_ref() {
                    entry.self_and_rivals_scroll_offset =
                        skin_scroll_offset_from_rate(value, ranking.entries.len());
                }
            }
        }
    }

    pub fn course_snapshot_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<&SelectCourseIrTarget>,
    ) -> ResultIrSnapshot {
        if enabled_provider(ir_config).is_none() {
            return ResultIrSnapshot::default();
        }
        let Some(target) = selected else {
            return snapshot_with_provider(ResultIrSnapshot::default(), ir_config);
        };
        let mut snapshot = self
            .course_cache
            .get(target)
            .map(|entry| {
                let mut snapshot = entry
                    .ranking
                    .as_ref()
                    .map(|ranking| {
                        result_ir_ranking_to_skin_snapshot_at(ranking, entry.scroll_offset)
                    })
                    .unwrap_or_else(|| entry.ir.clone());
                match snapshot.state {
                    SkinIrState::Loaded => {
                        snapshot.connect_success_ms = Some(elapsed_since_ms(entry.completed_at));
                    }
                    SkinIrState::Failed => {
                        snapshot.connect_fail_ms = Some(elapsed_since_ms(entry.completed_at));
                    }
                    _ => {}
                }
                snapshot
            })
            .unwrap_or_else(|| {
                let begin_ms = self
                    .course_in_flight
                    .as_ref()
                    .filter(|(_, current, _)| current == target)
                    .map(|(_, _, started_at)| elapsed_since_ms(*started_at));
                ResultIrSnapshot {
                    state: if begin_ms.is_some() {
                        SkinIrState::Loading
                    } else {
                        SkinIrState::Waiting
                    },
                    connect_begin_ms: begin_ms,
                    ..Default::default()
                }
            });
        snapshot.scope = SkinIrScope::Global;
        snapshot.global_scope_supported = true;
        snapshot.rival_scope_supported = false;
        snapshot_with_provider(snapshot, ir_config)
    }

    /// 選択中譜面のライバルベスト (最上位 1 名)。未取得 / IR 未設定なら None。
    pub fn rival_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
    ) -> Option<SelectRivalSnapshot> {
        enabled_provider(ir_config)?;
        if self.active_rival.is_some() {
            let sha256 = selected?;
            let score = self
                .active_rival_scores
                .iter()
                .find_map(|((hash, _), score)| (*hash == sha256).then_some(score))?;
            return Some(SelectRivalSnapshot {
                display_name: self.active_rival.as_ref()?.display_name.clone(),
                ex_score: score.ex_score,
                clear_index: rival_clear_index(score.clear_type),
                max_combo: score.max_combo,
                bp: score.min_bp.max(0) as u32,
                judge_counts: None,
            });
        }
        self.cache.get(&selected?).and_then(|entry| entry.rival.clone())
    }

    pub fn active_rival_name(&self) -> Option<&str> {
        self.active_rival.as_ref().map(|rival| rival.display_name.as_str())
    }

    pub fn active_rival_score(
        &self,
        chart_sha256: [u8; 32],
        ln_mode: u8,
    ) -> Option<&IrRivalScoreRecord> {
        self.active_rival_scores.get(&(chart_sha256, ln_mode))
    }

    pub fn active_rival_snapshot(
        &self,
        chart_sha256: [u8; 32],
        ln_mode: u8,
    ) -> Option<SelectRivalSnapshot> {
        let score = self.active_rival_score(chart_sha256, ln_mode)?;
        Some(SelectRivalSnapshot {
            display_name: self.active_rival.as_ref()?.display_name.clone(),
            ex_score: score.ex_score,
            clear_index: rival_clear_index(score.clear_type),
            max_combo: score.max_combo,
            bp: score.min_bp.max(0) as u32,
            judge_counts: None,
        })
    }

    pub fn active_rival_display_name(&self) -> Option<&str> {
        self.active_rival_name()
    }

    pub fn resolved_target_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
        target: TargetOption,
        local_best_ex_score: Option<u32>,
    ) -> Option<ResolvedTarget> {
        enabled_provider(ir_config)?;
        let entry = self.cache.get(&selected?)?;
        match target {
            TargetOption::IrTop => entry.global_targets.first().cloned(),
            TargetOption::IrNext => {
                next_target_above(&entry.global_targets, local_best_ex_score.unwrap_or(0))
            }
            TargetOption::RivalTop => entry.rival_targets.first().cloned(),
            TargetOption::RivalNext => {
                next_target_above(&entry.rival_targets, local_best_ex_score.unwrap_or(0))
            }
            TargetOption::RivalIndex(index) => {
                entry.rival_targets.get(index.saturating_sub(1) as usize).cloned()
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn target_ex_score_for(
        &self,
        ir_config: &IrConfig,
        selected: Option<[u8; 32]>,
        target: TargetOption,
        local_best_ex_score: Option<u32>,
    ) -> Option<u32> {
        self.resolved_target_for(ir_config, selected, target, local_best_ex_score)
            .map(|target| target.ex_score)
    }

    /// ログイン状態が変わったとき等にキャッシュを破棄する。
    pub fn clear(&mut self) {
        self.cache.clear();
        self.in_flight = None;
        self.pending = None;
        self.course_cache.clear();
        self.course_in_flight = None;
        self.course_pending = None;
        self.active_scope = SelectIrRankingScope::Global;
    }

    pub(super) fn insert_entry(&mut self, sha256: [u8; 32], entry: CachedChartIr) {
        if self.cache.len() >= CACHE_CAPACITY && !self.cache.contains_key(&sha256) {
            self.cache.clear();
        }
        self.cache.insert(sha256, entry);
        if self.in_flight.as_ref().is_some_and(|(_, in_flight_sha, _)| *in_flight_sha == sha256) {
            self.in_flight = None;
        }
        if self.pending.as_ref().is_some_and(|(pending_sha, _)| *pending_sha == sha256) {
            self.pending = None;
        }
    }
}

pub(super) fn rival_clear_index(clear_type: i32) -> i64 {
    i64::from(clear_type).clamp(0, bmz_core::clear::ClearType::Max as i64)
}

fn snapshot_with_provider(
    mut snapshot: ResultIrSnapshot,
    ir_config: &IrConfig,
) -> ResultIrSnapshot {
    let Some(provider) = crate::ir::provider_key::primary_provider_config(ir_config) else {
        return snapshot;
    };
    snapshot.online = true;
    if let Some(name) = crate::ir::provider_key::configured_provider_display_name(provider) {
        snapshot.provider_name = bmz_render::scene::ResultIrRankingName::from_display_name(name);
    }
    snapshot.user_name =
        bmz_render::scene::ResultIrRankingName::from_display_name(&provider.account_display_name);
    snapshot
}

fn skin_scroll_offset_from_rate(value: f32, entry_count: usize) -> usize {
    let max = entry_count.saturating_sub(bmz_render::scene::IR_RANKING_ENTRY_SLOTS);
    let rate = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
    ((rate * max as f32).round() as usize).min(max)
}
