use super::*;

impl SelectIrRanking {
    /// 選択中のrianIRライバルだけを全曲キャッシュする。切り替え直後は
    /// network.db の値を即時利用し、通信更新は起動中の取得条件ごとに1回だけ実行する。
    pub fn update_rival(&mut self, target: Option<SelectRivalFetchTarget>, profile_root: &Path) {
        while let Ok((result_target, requested_at, result)) = self.rival_receiver.try_recv() {
            if self.rival_in_flight.as_ref().is_some_and(|(target, _)| target == &result_target) {
                self.rival_in_flight = None;
            }
            if result.is_ok() {
                self.rival_fetched_this_session.insert((&result_target).into());
            }
            if self.active_rival.as_ref() != Some(&result_target) {
                tracing::debug!(rival = %result_target.rival_id, "discarding stale rival score fetch");
                continue;
            }
            match result {
                Ok(scores) => {
                    self.rival_pending = None;
                    self.active_rival_scores = scores
                        .into_iter()
                        .map(|score| ((score.chart_sha256, score.ln_mode), score))
                        .collect();
                    tracing::info!(
                        rival = %result_target.rival_id,
                        scores = self.active_rival_scores.len(),
                        elapsed_ms = requested_at.elapsed().as_millis(),
                        "rival score cache updated"
                    );
                }
                Err(error) => tracing::warn!(
                    rival = %result_target.rival_id,
                    %error,
                    "failed to refresh rival score cache; keeping local cache"
                ),
            }
        }

        if self.active_rival != target {
            self.active_rival = target.clone();
            self.active_rival_scores.clear();
            self.rival_pending = target
                .as_ref()
                .filter(|target| {
                    !self.rival_fetched_this_session.contains(&SelectRivalFetchKey::from(*target))
                })
                .map(|_| Instant::now());
            if let Some(target) = &target {
                let path = profile_root.join("network.db");
                match crate::storage::network_db::NetworkDatabase::open(&path).and_then(|db| {
                    db.rival_scores(&target.provider, &target.rival_id, &target.body)
                }) {
                    Ok(scores) => {
                        self.active_rival_scores = scores
                            .into_iter()
                            .map(|score| ((score.chart_sha256, score.ln_mode), score))
                            .collect();
                    }
                    Err(error) => tracing::debug!(%error, "rival score disk cache is unavailable"),
                }
            }
        }

        let Some(target) = target else {
            self.rival_pending = None;
            return;
        };
        if self.rival_in_flight.is_some() {
            return;
        }
        let Some(since) = self.rival_pending else {
            return;
        };
        if since.elapsed() < FETCH_DEBOUNCE {
            return;
        }
        self.rival_pending = None;
        let requested_at = Instant::now();
        self.rival_in_flight = Some((target.clone(), requested_at));
        spawn_rival_fetch(target, profile_root, requested_at, self.rival_sender.clone());
    }

    /// 毎フレーム呼ぶ。取得完了の取り込みと、カーソル譜面の取得予約を行う。
    /// `context` は rule mode / 解決済み LN policy / DOUBLE など
    /// ランキング条件を表す文字列。変わったらキャッシュを破棄する。
    pub fn update(
        &mut self,
        ir_config: &IrConfig,
        profile_root: &Path,
        context: &str,
        ln_policy: LnScorePolicy,
        double_option: DoubleOptionScoreBucket,
        rule_mode: RuleMode,
        selected: Option<[u8; 32]>,
    ) {
        if self.context != context {
            self.context = context.to_string();
            self.clear();
        }
        while let Ok((result_context, sha256, requested_at, result)) = self.receiver.try_recv() {
            if self.in_flight.as_ref().is_some_and(
                |(context, in_flight_sha, in_flight_requested_at)| {
                    context == &result_context
                        && *in_flight_sha == sha256
                        && *in_flight_requested_at == requested_at
                },
            ) {
                self.in_flight = None;
            }
            if result_context != self.context {
                tracing::debug!(
                    old_context = %result_context,
                    current_context = %self.context,
                    "discarding stale select IR ranking fetch"
                );
                continue;
            }
            if self.cache.get(&sha256).is_some_and(|entry| entry.completed_at >= requested_at) {
                tracing::debug!("discarding stale select IR ranking fetch");
                continue;
            }
            let completed_at = Instant::now();
            let entry = match result {
                Ok((global, self_and_rivals, rivals)) => {
                    let global_ranking = chart_ranking_to_result_ir_ranking(&global);
                    let self_and_rivals_ranking =
                        self_and_rivals.as_ref().map(chart_ranking_to_result_ir_ranking);
                    CachedChartIr {
                        global: ranking_to_ir_snapshot(&global),
                        global_ranking: Some(global_ranking),
                        global_scroll_offset: 0,
                        self_and_rivals: self_and_rivals.as_ref().map(ranking_to_ir_snapshot),
                        self_and_rivals_ranking,
                        self_and_rivals_scroll_offset: 0,
                        rival: rivals.as_ref().and_then(top_rival_snapshot),
                        global_battle_entries: battle_entries(&global),
                        self_and_rivals_battle_entries: self_and_rivals
                            .as_ref()
                            .map(battle_entries)
                            .unwrap_or_default(),
                        battle_entries_loaded: true,
                        global_targets: ranking_targets(&global),
                        rival_targets: rivals.as_ref().map(ranking_targets).unwrap_or_default(),
                        completed_at,
                    }
                }
                Err(error) => {
                    tracing::debug!(%error, "select IR ranking fetch failed");
                    CachedChartIr {
                        global: ResultIrSnapshot {
                            state: SkinIrState::Failed,
                            ..Default::default()
                        },
                        global_ranking: None,
                        global_scroll_offset: 0,
                        self_and_rivals: None,
                        self_and_rivals_ranking: None,
                        self_and_rivals_scroll_offset: 0,
                        rival: None,
                        global_battle_entries: Vec::new(),
                        self_and_rivals_battle_entries: Vec::new(),
                        battle_entries_loaded: true,
                        global_targets: Vec::new(),
                        rival_targets: Vec::new(),
                        completed_at,
                    }
                }
            };
            self.insert_entry(sha256, entry);
        }

        let Some(provider) = enabled_provider(ir_config) else {
            return;
        };
        let Some(sha256) = selected else {
            self.pending = None;
            return;
        };
        if self.cache.get(&sha256).is_some_and(|entry| entry.battle_entries_loaded)
            || self.in_flight.as_ref().is_some_and(|(_, in_flight_sha, _)| *in_flight_sha == sha256)
        {
            self.pending = None;
            return;
        }
        match self.pending {
            Some((pending_sha, since)) if pending_sha == sha256 => {
                if since.elapsed() >= FETCH_DEBOUNCE && self.in_flight.is_none() {
                    self.pending = None;
                    let requested_at = Instant::now();
                    self.in_flight = Some((self.context.clone(), sha256, requested_at));
                    spawn_fetch(
                        ResultIrQuery {
                            profile_root: profile_root.to_path_buf(),
                            provider: provider.0,
                            base_url: provider.1,
                            chart_sha256_hex: hash_to_hex(&sha256),
                            ln_policy,
                            double_option,
                            rule_mode,
                        },
                        self.context.clone(),
                        sha256,
                        requested_at,
                        self.sender.clone(),
                    );
                }
            }
            _ => self.pending = Some((sha256, Instant::now())),
        }
    }

    /// コース行用のグローバルIRランキングをデバウンス付きで取得する。
    pub fn update_course(
        &mut self,
        ir_config: &IrConfig,
        profile_root: &Path,
        context: &str,
        selected: Option<SelectCourseIrTarget>,
    ) {
        if self.context != context {
            self.context = context.to_string();
            self.clear();
        }
        while let Ok((result_context, target, requested_at, result)) =
            self.course_receiver.try_recv()
        {
            if self.course_in_flight.as_ref().is_some_and(|(context, current, _)| {
                context == &result_context && current == &target
            }) {
                self.course_in_flight = None;
            }
            if result_context != self.context {
                continue;
            }
            if self
                .course_cache
                .get(&target)
                .is_some_and(|entry| entry.completed_at >= requested_at)
            {
                continue;
            }
            let completed_at = Instant::now();
            let (ir, ranking) = match result {
                Ok(ranking) => {
                    let ranking = course_ranking_to_result_ir_ranking(&ranking);
                    (result_ir_ranking_to_skin_snapshot(&ranking), Some(ranking))
                }
                Err(error) => {
                    tracing::debug!(%error, "select course IR ranking fetch failed");
                    (ResultIrSnapshot { state: SkinIrState::Failed, ..Default::default() }, None)
                }
            };
            if self.course_cache.len() >= CACHE_CAPACITY && !self.course_cache.contains_key(&target)
            {
                self.course_cache.clear();
            }
            self.course_cache
                .insert(target, CachedCourseIr { ir, ranking, scroll_offset: 0, completed_at });
        }

        let Some(provider) = enabled_provider(ir_config) else {
            return;
        };
        let Some(target) = selected else {
            self.course_pending = None;
            return;
        };
        if self.course_cache.contains_key(&target)
            || self.course_in_flight.as_ref().is_some_and(|(_, current, _)| current == &target)
        {
            self.course_pending = None;
            return;
        }
        match &self.course_pending {
            Some((pending, since)) if pending == &target => {
                if since.elapsed() >= FETCH_DEBOUNCE && self.course_in_flight.is_none() {
                    self.course_pending = None;
                    let requested_at = Instant::now();
                    self.course_in_flight =
                        Some((self.context.clone(), target.clone(), requested_at));
                    spawn_course_fetch(
                        provider.0,
                        provider.1,
                        profile_root.to_path_buf(),
                        self.context.clone(),
                        target,
                        requested_at,
                        self.course_sender.clone(),
                    );
                }
            }
            _ => self.course_pending = Some((target, Instant::now())),
        }
    }

    /// Result 画面でスコア送信と同時に取得した Global ranking を選曲キャッシュへ反映する。
    pub fn cache_result_global_ranking(
        &mut self,
        chart_sha256_hex: &str,
        ranking: &ResultIrRanking,
    ) {
        if ranking.scope != IrRankingScope::Global {
            return;
        }
        let Ok(sha256) = hex_to_hash::<32>(chart_sha256_hex) else {
            tracing::warn!(
                chart = chart_sha256_hex,
                "discarding IR ranking for invalid chart hash"
            );
            return;
        };
        let (
            self_and_rivals,
            self_and_rivals_ranking,
            self_and_rivals_scroll_offset,
            rival,
            rival_targets,
            global_scroll_offset,
        ) = self
            .cache
            .get(&sha256)
            .map(|entry| {
                (
                    entry.self_and_rivals.clone(),
                    entry.self_and_rivals_ranking.clone(),
                    entry.self_and_rivals_scroll_offset,
                    entry.rival.clone(),
                    entry.rival_targets.clone(),
                    entry.global_scroll_offset,
                )
            })
            .unwrap_or_default();
        self.insert_entry(
            sha256,
            CachedChartIr {
                global: result_ir_ranking_to_skin_snapshot(ranking),
                global_ranking: Some(ranking.clone()),
                global_scroll_offset,
                self_and_rivals,
                self_and_rivals_ranking,
                self_and_rivals_scroll_offset,
                rival,
                // Result の表示用 ranking は G-BATTLE に必要な score identity を
                // 持たない。古い候補を残さず、Select 復帰時に完全な ranking を
                // 取得し直す。
                global_battle_entries: Vec::new(),
                self_and_rivals_battle_entries: Vec::new(),
                battle_entries_loaded: false,
                global_targets: result_ranking_targets(ranking),
                rival_targets,
                completed_at: Instant::now(),
            },
        );
        // プレイした譜面は既に選曲カーソルが安定しているため、通常のカーソル移動用
        // debounce を待たず、Select に戻った最初の update で再取得する。
        self.pending = Some((sha256, Instant::now() - FETCH_DEBOUNCE));
    }
}
