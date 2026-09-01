use super::*;

/// リザルト遷移時に呼ぶ。IR 未設定なら `None`。
///
/// 起動するタスク:
/// 1. pending スコアジョブの即時送信 (このリザルト分を含む)
/// 2. prefetch 設定が ON の scope のランキング取得
///
/// prefetch が両方 OFF でも、パネル表示時のタブ選択で遅延取得できる。
pub fn spawn_result_ir_task(
    profile_root: PathBuf,
    score_db_path: PathBuf,
    network_db_path: PathBuf,
    logs_dir: PathBuf,
    ir_config: &IrConfig,
    local_score_id: i64,
    chart_sha256_hex: String,
    ln_policy: LnScorePolicy,
    double_option: DoubleOptionScoreBucket,
    rule_mode: RuleMode,
) -> Option<ResultIrState> {
    spawn_result_ir_task_for_target(
        profile_root,
        score_db_path,
        network_db_path,
        logs_dir,
        ir_config,
        ResultIrTarget::Chart {
            local_score_id,
            chart_sha256_hex,
            ln_policy,
            double_option,
            rule_mode,
        },
    )
}

pub fn spawn_course_result_ir_task(
    profile_root: PathBuf,
    score_db_path: PathBuf,
    network_db_path: PathBuf,
    logs_dir: PathBuf,
    ir_config: &IrConfig,
    local_score_id: i64,
    hashes: ResultIrCourseHashes,
    gauge: String,
    ln_policy: String,
    rule_mode: RuleMode,
) -> Option<ResultIrState> {
    spawn_result_ir_task_for_target(
        profile_root,
        score_db_path,
        network_db_path,
        logs_dir,
        ir_config,
        ResultIrTarget::Course {
            local_score_id,
            course_hash: hashes.local,
            rian_course_hash_v1: hashes.rian_v1,
            bms_ir_course_key: hashes.bms_ir,
            gauge,
            ln_policy,
            rule_mode,
        },
    )
}

pub(super) fn spawn_result_ir_task_for_target(
    profile_root: PathBuf,
    score_db_path: PathBuf,
    network_db_path: PathBuf,
    logs_dir: PathBuf,
    ir_config: &IrConfig,
    target: ResultIrTarget,
) -> Option<ResultIrState> {
    let provider = crate::ir::provider_key::primary_provider_config(ir_config)?;
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)?;
    let query = ResultIrTaskQuery {
        profile_root,
        provider: provider_key.to_string(),
        account_id: provider.account_id.clone(),
        base_url: provider.base_url.clone(),
        target,
    };
    let mut submission_targets = Vec::new();
    let mut provider_submissions = Vec::new();
    for configured in &ir_config.providers {
        if !configured.enabled || configured.base_url.is_empty() {
            continue;
        }
        let Some(configured_key) = crate::ir::provider_key::configured_provider_key(configured)
        else {
            continue;
        };
        let Some(display_name) =
            crate::ir::provider_key::configured_provider_display_name(configured)
        else {
            continue;
        };
        submission_targets.push(ResultIrSubmissionTarget {
            provider: configured_key.to_string(),
            account_id: configured.account_id.clone(),
        });
        provider_submissions.push(ResultIrProviderSubmission {
            provider_key: configured_key.to_string(),
            display_name: display_name.to_string(),
            primary: configured_key == provider_key,
            state: IrSubmitState::Sending,
        });
    }
    let (sender, receiver) = channel();

    let mut state = ResultIrState {
        submit: IrSubmitState::Sending,
        provider_submissions,
        global: RankingLoadState::NotRequested,
        self_and_rivals: RankingLoadState::NotRequested,
        active_tab: ResultRankingTab::Global,
        ir_connect_begin_at: Some(Instant::now()),
        ir_connect_success_at: None,
        ir_connect_fail_at: None,
        provider_name: bmz_render::scene::ResultIrRankingName::from_display_name(
            crate::ir::provider_key::configured_provider_display_name(provider)?,
        ),
        user_name: bmz_render::scene::ResultIrRankingName::from_display_name(
            &provider.account_display_name,
        ),
        global_skin_scroll_offset: 0,
        self_and_rivals_skin_scroll_offset: 0,
        query: query.clone(),
        sender: sender.clone(),
        receiver,
    };

    let submit_sender = sender.clone();
    let ir_config = ir_config.clone();
    let submit_query = query.clone();
    let submission_targets_for_task = submission_targets.clone();
    // global は Result スキンの NUMBER_IR_RANK / OPTION_IR_* 表示にも使うため、
    // prefetch 設定に関わらず常に取得する。rivals scope のみ設定に従う。
    let prefetch_global = true;
    let prefetch_rivals =
        query.supports_scope(IrRankingScope::SelfAndRivals) && state_prefetch_rivals(&ir_config);
    tokio::spawn(async move {
        let now = now_unix_seconds();
        let primary_outcome = async {
            crate::storage::migration::migrate_network_db(&network_db_path)?;
            let mut network_db =
                crate::storage::network_db::NetworkDatabase::open(&network_db_path)?;
            let (kind, local_score_id) = submit_query.target.submission_job();
            sync_pending_ir_jobs_filtered(
                &mut network_db,
                &score_db_path,
                &submit_query.profile_root,
                &logs_dir,
                &ir_config,
                IrSyncJobFilter {
                    provider_key: &submit_query.provider,
                    account_id: &submit_query.account_id,
                    kind,
                    local_score_id: Some(local_score_id),
                },
                now,
                1,
                false,
                IrSyncThrottle::none(),
            )
            .await
        }
        .await;
        let mut included_global_ranking = None;
        match primary_outcome {
            Ok(report) => {
                let primary_processed = report.submitted.saturating_add(report.failed);
                included_global_ranking = included_global_ranking_for_query(&submit_query, &report);
                // 通常のランキングAPIには更新前順位が無い。今回の primary provider の
                // 送信応答を取得できた時点で先に Result へ渡し、残りの provider や
                // backlog の同期完了を待たせない。
                if let Some(ranking) = included_global_ranking.clone() {
                    let _ = submit_sender.send(ResultIrEvent::Ranking {
                        provider: submit_query.provider.clone(),
                        scope: IrRankingScope::Global,
                        result: Ok(ranking),
                    });
                }

                // primary provider の今回分を優先した後、従来どおり残りの pending
                // job も送る。primary の送信応答は上で確保済みなので、バッチ順や
                // 上限によって通常ランキング取得へ誤ってフォールバックしない。
                let remaining_outcome = async {
                    let mut network_db =
                        crate::storage::network_db::NetworkDatabase::open(&network_db_path)?;
                    sync_pending_ir_jobs(
                        &mut network_db,
                        &score_db_path,
                        &submit_query.profile_root,
                        &logs_dir,
                        &ir_config,
                        now_unix_seconds(),
                        IR_SYNC_BATCH_LIMIT.saturating_sub(primary_processed),
                        false,
                        IrSyncThrottle::rate_limited(),
                    )
                    .await
                }
                .await;
                if let Err(error) = remaining_outcome {
                    tracing::warn!(%error, "failed to sync remaining IR jobs from Result");
                }
                // 別の同期 task がこの job を先に claim していても、送信完了まで
                // 待ってから ranking を取得する。これで古いサーバ側 ranking を
                // Result に固定しない。
                for event in watch_result_submissions(
                    &network_db_path,
                    &submit_query,
                    &submission_targets_for_task,
                )
                .await
                {
                    let _ = submit_sender.send(event);
                }
                if included_global_ranking.is_none() {
                    match stored_included_global_ranking(&network_db_path, &submit_query) {
                        Ok(Some(ranking)) => {
                            let _ = submit_sender.send(ResultIrEvent::Ranking {
                                provider: submit_query.provider.clone(),
                                scope: IrRankingScope::Global,
                                result: Ok(ranking.clone()),
                            });
                            included_global_ranking = Some(ranking);
                        }
                        Ok(None) => {}
                        Err(error) => tracing::warn!(
                            %error,
                            "failed to load the completed IR submission response",
                        ),
                    }
                }
            }
            Err(error) => {
                let message = format!("{error:#}");
                for target in &submission_targets_for_task {
                    let _ = submit_sender.send(ResultIrEvent::Submit {
                        provider: target.provider.clone(),
                        submitted: 0,
                        failed: 0,
                        message: Some(message.clone()),
                    });
                }
            }
        }
        let included_global_loaded = included_global_ranking.is_some();
        // 送信完了後に prefetch する。best 更新前のランキングを返さないため。
        if prefetch_global && !included_global_loaded {
            fetch_ranking_and_send(&submit_query, IrRankingScope::Global, &submit_sender).await;
        }
        if prefetch_rivals {
            fetch_ranking_and_send(&submit_query, IrRankingScope::SelfAndRivals, &submit_sender)
                .await;
        }
    });

    if prefetch_global {
        state.global = RankingLoadState::Loading;
    }
    if prefetch_rivals {
        state.self_and_rivals = RankingLoadState::Loading;
    }
    Some(state)
}

/// 常駐同期との claim race があっても、今回の attempt の provider 別終端状態を待つ。
pub(super) async fn watch_result_submissions(
    network_db_path: &std::path::Path,
    query: &ResultIrTaskQuery,
    targets: &[ResultIrSubmissionTarget],
) -> Vec<ResultIrEvent> {
    const POLL_INTERVAL: Duration = Duration::from_millis(250);
    const MAX_POLLS: usize = 120;
    let (kind, local_score_id) = query.target.submission_job();
    let mut latest_jobs = Vec::new();

    for _ in 0..MAX_POLLS {
        match crate::storage::network_db::NetworkDatabase::open(network_db_path)
            .and_then(|db| db.ir_score_jobs_for_local_score(kind, local_score_id))
        {
            Ok(jobs) => {
                latest_jobs = jobs;
                let mut events = Vec::with_capacity(targets.len());
                let mut all_finished = true;
                for target in targets {
                    let provider_jobs = latest_jobs
                        .iter()
                        .filter(|job| {
                            job.provider == target.provider && job.account_id == target.account_id
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    if let Some((submitted, failed, message)) =
                        submission_result_from_jobs(&provider_jobs)
                    {
                        events.push(ResultIrEvent::Submit {
                            provider: target.provider.clone(),
                            submitted,
                            failed,
                            message,
                        });
                    } else {
                        all_finished = false;
                    }
                }
                if all_finished {
                    return events;
                }
            }
            Err(error) => {
                let message = format!("failed to read IR submission status: {error:#}");
                return targets
                    .iter()
                    .map(|target| ResultIrEvent::Submit {
                        provider: target.provider.clone(),
                        submitted: 0,
                        failed: 0,
                        message: Some(message.clone()),
                    })
                    .collect();
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    targets
        .iter()
        .map(|target| {
            let provider_jobs = latest_jobs
                .iter()
                .filter(|job| {
                    job.provider == target.provider && job.account_id == target.account_id
                })
                .cloned()
                .collect::<Vec<_>>();
            let (submitted, failed, message) = submission_result_from_jobs(&provider_jobs)
                .unwrap_or_else(|| (0, 0, Some("timed out waiting for IR submission".to_string())));
            ResultIrEvent::Submit { provider: target.provider.clone(), submitted, failed, message }
        })
        .collect()
}

pub(super) fn submission_result_from_jobs(
    jobs: &[IrScoreJobRecord],
) -> Option<(u32, u32, Option<String>)> {
    if jobs.is_empty() {
        return Some((0, 0, None));
    }
    let failed: Vec<_> =
        jobs.iter().filter(|job| job.status == IrScoreJobStatus::Failed.as_str()).collect();
    if !failed.is_empty() {
        return Some((
            0,
            failed.len() as u32,
            failed
                .iter()
                .find_map(|job| (!job.last_error.is_empty()).then(|| job.last_error.clone())),
        ));
    }
    if jobs.iter().all(|job| job.status == IrScoreJobStatus::Succeeded.as_str()) {
        return Some((jobs.len() as u32, 0, None));
    }
    None
}

pub(super) fn elapsed_since_ms(started_at: Instant) -> i32 {
    started_at.elapsed().as_millis().min(i32::MAX as u128) as i32
}

pub(super) fn state_prefetch_rivals(ir_config: &IrConfig) -> bool {
    ir_config.prefetch_rival_ranking_on_score_submit
}

pub(super) fn included_global_ranking_for_query(
    query: &ResultIrTaskQuery,
    report: &IrSyncReport,
) -> Option<ResultIrRanking> {
    let ResultIrTarget::Chart { local_score_id, chart_sha256_hex, .. } = &query.target else {
        return None;
    };
    report
        .included_rankings
        .iter()
        .find(|ranking| {
            ranking.provider == query.provider
                && ranking.account_id == query.account_id
                && ranking.kind == IrJobKind::Score
                && ranking.local_score_id == *local_score_id
                && ranking.ranking.chart.sha256 == *chart_sha256_hex
                && ranking.ranking.ranking.scope == IrRankingScope::Global
        })
        .map(|ranking| {
            chart_ranking_to_result_ir_ranking_with_previous(
                &ranking.ranking,
                ranking.previous_rank,
            )
        })
}

fn stored_included_global_ranking(
    network_db_path: &std::path::Path,
    query: &ResultIrTaskQuery,
) -> anyhow::Result<Option<ResultIrRanking>> {
    let ResultIrTarget::Chart { local_score_id, .. } = &query.target else {
        return Ok(None);
    };
    let response_json = crate::storage::network_db::NetworkDatabase::open(network_db_path)?
        .latest_ir_score_submission_response(
            &query.provider,
            &query.account_id,
            IrJobKind::Score,
            *local_score_id,
        )?;
    let Some(response_json) = response_json else {
        return Ok(None);
    };
    let response = serde_json::from_str::<IrSubmitResponse>(&response_json)?;
    Ok(included_global_ranking_from_response(query, &response))
}

pub(super) fn included_global_ranking_from_response(
    query: &ResultIrTaskQuery,
    response: &IrSubmitResponse,
) -> Option<ResultIrRanking> {
    let ResultIrTarget::Chart { chart_sha256_hex, .. } = &query.target else {
        return None;
    };
    let ranking =
        response.rankings.get(&IrRankingScope::Global).filter(|ranking| ranking.succeeded)?;
    let data = ranking.data.as_ref().filter(|data| data.chart.sha256 == *chart_sha256_hex)?;
    Some(chart_ranking_to_result_ir_ranking_with_previous(data, ranking.previous_rank))
}

pub(super) fn spawn_ranking_fetch(
    query: ResultIrTaskQuery,
    scope: IrRankingScope,
    sender: Sender<ResultIrEvent>,
) {
    tokio::spawn(async move {
        fetch_ranking_and_send(&query, scope, &sender).await;
    });
}

pub(super) async fn fetch_ranking_and_send(
    query: &ResultIrTaskQuery,
    scope: IrRankingScope,
    sender: &Sender<ResultIrEvent>,
) {
    let result = fetch_result_ranking(query, scope).await.map_err(|error| format!("{error:#}"));
    let _ = sender.send(ResultIrEvent::Ranking { provider: query.provider.clone(), scope, result });
}

pub(super) async fn fetch_result_ranking(
    query: &ResultIrTaskQuery,
    scope: IrRankingScope,
) -> anyhow::Result<ResultIrRanking> {
    match &query.target {
        ResultIrTarget::Chart { chart_sha256_hex, ln_policy, double_option, rule_mode, .. } => {
            let ranking = fetch_ranking(
                &ResultIrQuery {
                    profile_root: query.profile_root.clone(),
                    provider: query.provider.clone(),
                    base_url: query.base_url.clone(),
                    chart_sha256_hex: chart_sha256_hex.clone(),
                    ln_policy: *ln_policy,
                    double_option: *double_option,
                    rule_mode: *rule_mode,
                },
                scope,
            )
            .await?;
            Ok(chart_ranking_to_result_ir_ranking(&ranking))
        }
        ResultIrTarget::Course {
            course_hash,
            rian_course_hash_v1,
            bms_ir_course_key,
            gauge,
            ln_policy,
            rule_mode,
            ..
        } => {
            if scope != IrRankingScope::Global {
                anyhow::bail!("course IR ranking supports global scope only");
            }
            if crate::ir::rian_ir::is_rian_ir_provider(&query.provider) {
                return crate::ir::rian_ir::RianIrClient::new(&query.base_url)?
                    .fetch_course_ranking(
                        rian_course_hash_v1,
                        crate::ir::rian_ir::body_for_rule_mode(*rule_mode),
                        crate::ir::rian_ir::RIAN_IR_RANKING_LIMIT,
                    )
                    .await
                    .map(|ranking| course_ranking_to_result_ir_ranking(&ranking));
            }
            if crate::ir::bms_ir::is_bms_ir_provider(&query.provider) {
                let credentials = ensure_fresh_credentials(
                    &query.profile_root,
                    &query.provider,
                    &query.base_url,
                    now_unix_seconds(),
                )
                .await?;
                return crate::ir::bms_ir::BmsIrClient::new(&query.base_url)?
                    .fetch_course_ranking(
                        course_hash,
                        bms_ir_course_key.as_deref().unwrap_or(""),
                        &IrCourseRankingRequest {
                            gauge: gauge.clone(),
                            ln_policy: ln_policy.clone(),
                            limit: 20,
                        },
                        *rule_mode,
                        &credentials.account_id,
                        &credentials.access_token,
                    )
                    .await
                    .map(|ranking| course_ranking_to_result_ir_ranking(&ranking));
            }
            let client = BmzOfficialIrClient::anonymous(&query.base_url)?;
            let ranking = client
                .fetch_course_ranking(
                    course_hash,
                    &IrCourseRankingRequest {
                        gauge: gauge.clone(),
                        ln_policy: ln_policy.clone(),
                        limit: 20,
                    },
                )
                .await?;
            Ok(course_ranking_to_result_ir_ranking(&ranking))
        }
    }
}

pub(crate) async fn fetch_ranking(
    query: &ResultIrQuery,
    scope: IrRankingScope,
) -> anyhow::Result<IrRankingResult> {
    let limit = result_ranking_limit(&query.provider);
    fetch_ranking_with_limit(query, scope, limit).await
}

pub(super) fn result_ranking_limit(provider: &str) -> u32 {
    if crate::ir::rian_ir::is_rian_ir_provider(provider) {
        crate::ir::rian_ir::RIAN_IR_RANKING_LIMIT
    } else {
        crate::ir::types::default_ranking_limit()
    }
}

pub(crate) async fn fetch_ranking_with_limit(
    query: &ResultIrQuery,
    scope: IrRankingScope,
    limit: u32,
) -> anyhow::Result<IrRankingResult> {
    let now = now_unix_seconds();
    if crate::ir::rian_ir::is_rian_ir_provider(&query.provider) {
        let credentials =
            ensure_fresh_credentials(&query.profile_root, &query.provider, &query.base_url, now)
                .await
                .ok();
        return crate::ir::rian_ir::RianIrClient::new(&query.base_url)?
            .fetch_ranking(
                &query.chart_sha256_hex,
                crate::ir::rian_ir::body_for_rule_mode(query.rule_mode),
                scope,
                limit.min(crate::ir::rian_ir::RIAN_IR_RANKING_LIMIT),
                credentials.as_ref().map(|credentials| credentials.account_id.as_str()),
            )
            .await;
    }
    if crate::ir::bms_ir::is_bms_ir_provider(&query.provider) {
        let credentials =
            ensure_fresh_credentials(&query.profile_root, &query.provider, &query.base_url, now)
                .await?;
        return crate::ir::bms_ir::BmsIrClient::new(&query.base_url)?
            .fetch_ranking(
                &query.chart_sha256_hex,
                &IrRankingRequest {
                    scope,
                    ln_policy: query.ln_policy.as_str().to_string(),
                    double_option: query.double_option,
                    rule_mode: query.rule_mode,
                    limit,
                    offset: 0,
                },
                &credentials.account_id,
                &credentials.access_token,
            )
            .await;
    }
    let mut client = BmzOfficialIrClient::anonymous(&query.base_url)?;
    // self / rivals scope は認証必須。global は匿名でも可。
    match ensure_fresh_credentials(&query.profile_root, &query.provider, &query.base_url, now).await
    {
        Ok(credentials) => client.set_access_token(credentials.access_token),
        Err(error) if scope != IrRankingScope::Global => return Err(error),
        Err(_) => {}
    }
    client
        .fetch_ranking(
            &query.chart_sha256_hex,
            &IrRankingRequest {
                scope,
                ln_policy: query.ln_policy.as_str().to_string(),
                double_option: query.double_option,
                rule_mode: query.rule_mode,
                limit,
                offset: 0,
            },
        )
        .await
}

pub(super) fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
