import { randomUUID } from 'node:crypto'
import { and, desc, eq, inArray, ne, or, sql, type SQL } from 'drizzle-orm'
import type { BatchItem } from 'drizzle-orm/batch'
import type { AnySQLiteColumn } from 'drizzle-orm/sqlite-core'
import { db, schema } from 'hub:db'
import { isUniqueConstraintError } from '../../utils/db_errors'
import { assertCanSubmit, assertCanWriteScore } from '../moderation/enforcement'
import { assertIdempotentSubmission } from './idempotency'
import type {
  IrChartLnProfile,
  IrDoubleOption,
  IrRankingScope,
  IrRuleMode,
  IrScoreSubmission,
  IrSubmitResponse,
  IrVerificationStatus,
  LnScorePolicy,
} from '../../../shared/types/ir'

import {
  CLEAR_RANK,
  LOCAL_BACKFILL_SOURCE,
  MAX_LOCAL_BACKFILL_DELETE_BATCH_SIZE,
  type BestScoreCandidate,
  type BestScoreKey,
  IrBackfillCleanupError,
  IrEvidenceValidationError,
  IrScoreNotFoundError,
  type IrRequestUser,
  type LocalBackfillDeleteResult,
  type ScoreAttestationPayload,
  bestCandidateWins,
  judgeTotal,
  normalizeDoubleOption,
  playedAtDate,
  scoreSubmissionMetadata,
} from './common'
import {
  bestRowsFromHistory,
  getRanking,
  getRankingWithPreviousRank,
  rowToBestScoreRow,
} from './ranking'
import { resolveVerification, verificationStatusForSignedSubmission } from './verification'

export async function attestScore(
  user: IrRequestUser,
  scoreId: string,
  payload: ScoreAttestationPayload,
): Promise<{ score_id: string; verification: IrVerificationStatus }> {
  if (payload.score_id !== scoreId) {
    throw new IrEvidenceValidationError('score_id does not match the request path')
  }
  await assertCanWriteScore(user.id, scoreId)
  const attestationVerification = await resolveVerification(user.id, payload)
  if (attestationVerification === 'unverified') {
    throw new IrEvidenceValidationError('score attestation evidence is required')
  }

  const score = await db.query.scores.findFirst({
    columns: { evidence: true, playOptions: true },
    where: and(eq(schema.scores.id, scoreId), eq(schema.scores.playerId, user.id)),
  })
  if (!score) {
    throw new IrScoreNotFoundError('score not found')
  }

  const verification = verificationStatusForSignedSubmission({ play_options: score.playOptions })
  const evidence = { ...score.evidence, attestation: payload.evidence }
  await db.batch([
    db
      .update(schema.scores)
      .set({ evidence, verification })
      .where(and(eq(schema.scores.id, scoreId), eq(schema.scores.playerId, user.id))),
    db
      .update(schema.bestScores)
      .set({ verification, updatedAt: new Date() })
      .where(and(eq(schema.bestScores.scoreId, scoreId), eq(schema.bestScores.playerId, user.id))),
  ])
  return { score_id: scoreId, verification }
}

export async function submitScore(
  user: IrRequestUser,
  payload: IrScoreSubmission,
  rankingScopes: IrRankingScope[],
  rankingLimit: number,
): Promise<IrSubmitResponse> {
  const { doubleOption, appliedDoubleOption, sourceKind } = scoreSubmissionMetadata(
    payload.play_options,
  )

  // ユーザー・譜面の状態は idempotency key の確認より先に判定する。BAN・ブロック前に
  // 保存済みのスコアを再送しても、成功応答やランキングを返さない。
  await assertCanSubmit(user.id, [payload.chart.sha256])

  // 同一 idempotency key の再送は、当時の evidence 形式がすでに廃止されていても
  // 保存済み score を成功として返す。初回送信の検証・保存には到達させない。
  const existing = await findIdempotentScore(user.id, payload.idempotency_key)
  if (existing) {
    assertIdempotentSubmission(existing, user.id, payload)
    let previousBestExScore: number | null | undefined
    try {
      previousBestExScore = await fetchPreviousBestExScoreExcluding(user.id, payload, existing.id)
    } catch (error) {
      console.error('failed to rebuild previous IR rank for idempotent score', error)
    }
    const rankings = await submissionRankings(
      user,
      payload,
      rankingScopes,
      rankingLimit,
      doubleOption,
      previousBestExScore,
    )
    return idempotentScoreResponse(existing, rankings)
  }

  const verification = await resolveVerification(user.id, payload)
  await upsertChart(payload, shouldUpdateExistingChart(payload.play_options, doubleOption))

  const bp =
    judgeTotal(payload, 'bad') + judgeTotal(payload, 'poor') + judgeTotal(payload, 'empty_poor')
  const cb = judgeTotal(payload, 'bad') + judgeTotal(payload, 'poor')
  const clearRank = CLEAR_RANK[payload.result.clear] ?? 0
  const deviceType = payload.play_options.device_type

  const scoreId = randomUUID()
  // best 更新と同一 batch で atomic に書くため、DB default に任せず
  // アプリ側で受信時刻を確定させる。
  const serverReceivedAt = new Date()
  const scoreInsert = {
    id: scoreId,
    playerId: user.id,
    chartSha256: payload.chart.sha256,
    clientName: payload.client.name,
    clientVersion: payload.client.version,
    platform: payload.client.platform,
    playMode: payload.rule.play_mode,
    keyMode: payload.rule.key_mode,
    gauge: payload.rule.gauge,
    lnPolicy: payload.rule.ln_policy,
    effectiveLnMode: payload.rule.effective_ln_mode,
    ruleMode: payload.rule.rule_mode,
    judgeAlgorithm: payload.rule.judge_algorithm,
    scoring: payload.rule.scoring,
    clearType: payload.result.clear,
    clearRank,
    playedAt: playedAtDate(payload.result.played_at),
    durationMs: payload.result.duration_ms ?? null,
    judges: payload.result.judges,
    exScore: payload.result.ex_score,
    avgJudgeMs: payload.result.avg_judge_ms ?? null,
    maxCombo: payload.result.max_combo,
    notes: payload.result.notes,
    passNotes: payload.result.pass_notes ?? payload.result.notes,
    bp,
    cb,
    minBp: payload.result.min_bp,
    minCb: payload.result.min_cb,
    deviceType,
    doubleOption,
    appliedDoubleOption,
    sourceKind,
    playOptions: {
      ...payload.play_options,
      double_option: doubleOption,
      applied_double_option: appliedDoubleOption,
      source_kind: sourceKind,
    } as Record<string, unknown>,
    replayHash: payload.replay?.hash ?? null,
    replayFormat: payload.replay?.format ?? null,
    replayUploadIntent: payload.replay?.upload_intent ?? null,
    evidence: payload.evidence ?? {},
    verification,
    idempotencyKey: payload.idempotency_key,
    serverReceivedAt,
  }

  const previousBest = await fetchPreviousBest(user.id, payload)

  // score insert と best 更新は D1 batch (implicit transaction) で atomic に
  // 書く。挿入と best 更新の間で Worker が落ちても不整合が残らない。
  const candidate: BestScoreCandidate = {
    ex_score: payload.result.ex_score,
    clear_rank: clearRank,
    max_combo: payload.result.max_combo,
    min_bp: payload.result.min_bp,
    min_cb: payload.result.min_cb,
    server_received_at: serverReceivedAt,
  }
  const score = { id: scoreId, serverReceivedAt }
  const best = await prepareBestScoreUpsert(user.id, payload, scoreId, verification, candidate)
  try {
    const insertStatement = db.insert(schema.scores).values(scoreInsert)
    if (best.statement) {
      await db.batch([insertStatement, best.statement])
    } else {
      await insertStatement
    }
  } catch (error) {
    if (!isUniqueConstraintError(error)) {
      throw error
    }
    // 初回送信が同時に確定した競合。既存 score を返すだけにして、
    // 再送 payload で best score を再計算・上書きしない。
    const existing = await findIdempotentScore(user.id, payload.idempotency_key)
    if (!existing) {
      throw new Error('failed to insert score', { cause: error })
    }
    assertIdempotentSubmission(existing, user.id, payload)
    let previousBestExScore: number | null | undefined
    try {
      previousBestExScore = await fetchPreviousBestExScoreExcluding(user.id, payload, existing.id)
    } catch (rankingError) {
      console.error('failed to rebuild previous IR rank after concurrent submission', rankingError)
    }
    const rankings = await submissionRankings(
      user,
      payload,
      rankingScopes,
      rankingLimit,
      doubleOption,
      previousBestExScore,
    )
    return idempotentScoreResponse(existing, rankings)
  }
  const { bestUpdated, updatedFields } = best

  const rankings = await submissionRankings(
    user,
    payload,
    rankingScopes,
    rankingLimit,
    doubleOption,
    previousBest?.ex_score ?? null,
  )

  return {
    accepted: true,
    score_id: score.id,
    best_updated: bestUpdated,
    updated_fields: updatedFields,
    server_received_at: score.serverReceivedAt.toISOString(),
    previous_best: previousBest,
    rankings: Object.keys(rankings).length > 0 ? rankings : undefined,
  }
}

async function submissionRankings(
  user: IrRequestUser,
  payload: IrScoreSubmission,
  rankingScopes: IrRankingScope[],
  rankingLimit: number,
  doubleOption: IrDoubleOption,
  previousBestExScore: number | null | undefined,
): Promise<NonNullable<IrSubmitResponse['rankings']>> {
  const rankings: NonNullable<IrSubmitResponse['rankings']> = {}
  for (const scope of rankingScopes) {
    try {
      const query = {
        scope,
        limit: rankingLimit,
        offset: 0,
        lnPolicy: payload.rule.ln_policy,
        doubleOption,
        ruleMode: payload.rule.rule_mode,
        scoring: payload.rule.scoring,
      }
      if (previousBestExScore === undefined) {
        rankings[scope] = {
          succeeded: true,
          data: await getRanking(user, payload.chart.sha256, query),
        }
      } else {
        const result = await getRankingWithPreviousRank(
          user,
          payload.chart.sha256,
          query,
          previousBestExScore,
        )
        rankings[scope] = {
          succeeded: true,
          previous_rank: result.previousRank,
          data: result.data,
        }
      }
    } catch (error) {
      rankings[scope] = {
        succeeded: false,
        error: error instanceof Error ? error.message : 'ranking failed',
      }
    }
  }
  return rankings
}

/**
 * 古い local_backfill 行を本人だけが削除するための保守API。
 *
 * score_history の再importで正しい metadata を持つ行を作り直す用途に限定し、
 * 通常プレイや別プレイヤーのscoreを削除できないようにする。
 */
export async function deleteLocalBackfillScores(
  user: IrRequestUser,
  requestedScoreIds: string[],
): Promise<LocalBackfillDeleteResult> {
  const scoreIds = [...new Set(requestedScoreIds.map((id) => id.trim()).filter(Boolean))]
  if (scoreIds.length === 0) {
    throw new IrBackfillCleanupError('score_ids must not be empty')
  }
  if (scoreIds.length > MAX_LOCAL_BACKFILL_DELETE_BATCH_SIZE) {
    throw new IrBackfillCleanupError(
      `score_ids must contain at most ${MAX_LOCAL_BACKFILL_DELETE_BATCH_SIZE} entries`,
    )
  }

  const rows = await db
    .select({
      id: schema.scores.id,
      chart_sha256: schema.scores.chartSha256,
      ln_policy: schema.scores.lnPolicy,
      double_option: schema.scores.doubleOption,
      rule_mode: schema.scores.ruleMode,
      scoring: schema.scores.scoring,
      play_options: schema.scores.playOptions,
    })
    .from(schema.scores)
    .where(and(eq(schema.scores.playerId, user.id), inArray(schema.scores.id, scoreIds)))

  const foundIds = new Set(rows.map((row) => row.id))
  const { localBackfillRows, retainedScoreIds } = partitionLocalBackfillRows(rows)
  const deletedScoreIds = localBackfillRows.map((row) => row.id)
  if (deletedScoreIds.length === 0) {
    return {
      deleted_score_ids: [],
      missing_score_ids: scoreIds.filter((id) => !foundIds.has(id)),
      retained_score_ids: retainedScoreIds,
    }
  }
  const affectedKeys = uniqueBestScoreKeys(localBackfillRows)
  const affectedConditions = affectedKeys.map(bestScoreKeyCondition)
  const affectedWhere = and(eq(schema.bestScores.playerId, user.id), or(...affectedConditions))
  const remainingWhere = and(
    eq(schema.scores.playerId, user.id),
    eq(schema.scores.accepted, true),
    or(...affectedKeys.map(scoreHistoryKeyCondition)),
  )

  const deletedScoreIdSet = new Set(deletedScoreIds)
  const remainingRows = await db
    .select({
      id: schema.scores.id,
      player_id: schema.scores.playerId,
      chart_sha256: schema.scores.chartSha256,
      ex_score: schema.scores.exScore,
      clear_type: schema.scores.clearType,
      clear_rank: schema.scores.clearRank,
      max_combo: schema.scores.maxCombo,
      min_bp: schema.scores.minBp,
      min_cb: schema.scores.minCb,
      device_type: schema.scores.deviceType,
      gauge: schema.scores.gauge,
      ln_policy: schema.scores.lnPolicy,
      effective_ln_mode: schema.scores.effectiveLnMode,
      double_option: schema.scores.doubleOption,
      rule_mode: schema.scores.ruleMode,
      scoring: schema.scores.scoring,
      played_at: schema.scores.playedAt,
      server_received_at: schema.scores.serverReceivedAt,
      verification: schema.scores.verification,
      play_options: schema.scores.playOptions,
    })
    .from(schema.scores)
    .where(remainingWhere)

  const rebuilt = bestRowsFromHistory(
    remainingRows
      .filter((row) => !deletedScoreIdSet.has(row.id))
      .map((row) => ({ ...rowToBestScoreRow({ ...row, score_id: row.id }), id: row.id })),
  )
  const updatedAt = new Date()
  const statements = [
    // D1 は明示 transaction の BEGIN を受け付けない。batch は原子的に実行されるため、
    // score を削除すると source score を参照する best_scores は cascade され得る。
    // affected key 全体を同じ batch で組み直し、残存scoreから正しい代表行を復元する。
    db.delete(schema.bestScores).where(affectedWhere),
    db
      .delete(schema.scores)
      .where(and(eq(schema.scores.playerId, user.id), inArray(schema.scores.id, deletedScoreIds))),
    ...rebuilt.map((row) =>
      db.insert(schema.bestScores).values({
        id: randomUUID(),
        playerId: row.player_id,
        chartSha256: row.chart_sha256,
        scoreId: row.score_id,
        bestExScoreId: row.best_ex_score_id,
        bestClearScoreId: row.best_clear_score_id,
        bestMaxComboScoreId: row.best_max_combo_score_id,
        bestMinBpScoreId: row.best_min_bp_score_id,
        bestMinCbScoreId: row.best_min_cb_score_id,
        exScore: row.ex_score,
        clearType: row.clear_type,
        clearRank: row.clear_rank,
        maxCombo: row.max_combo,
        minBp: row.min_bp,
        minCb: row.min_cb,
        deviceType: row.device_type,
        doubleOption: row.double_option,
        gauge: row.gauge,
        lnPolicy: row.ln_policy,
        effectiveLnMode: row.effective_ln_mode,
        ruleMode: row.rule_mode,
        scoring: row.scoring,
        playedAt: row.played_at ? new Date(row.played_at) : null,
        serverReceivedAt: row.server_received_at,
        verification: row.verification,
        updatedAt,
      }),
    ),
  ] as [BatchItem<'sqlite'>, ...BatchItem<'sqlite'>[]]
  await db.batch(statements)

  return {
    deleted_score_ids: deletedScoreIds,
    missing_score_ids: scoreIds.filter((id) => !foundIds.has(id)),
    retained_score_ids: retainedScoreIds,
  }
}

export function partitionLocalBackfillRows<
  TRow extends { id: string; play_options: Record<string, unknown> | null },
>(rows: TRow[]): { localBackfillRows: TRow[]; retainedScoreIds: string[] } {
  const localBackfillRows = rows.filter(
    (row) => row.play_options?.submission_source === LOCAL_BACKFILL_SOURCE,
  )
  return {
    localBackfillRows,
    // Legacy cleanup candidates can include a locally linked, verified normal play.
    // Never delete it remotely merely because its local score_history row was reimported.
    retainedScoreIds: rows
      .filter((row) => row.play_options?.submission_source !== LOCAL_BACKFILL_SOURCE)
      .map((row) => row.id),
  }
}

export function uniqueBestScoreKeys(
  rows: Array<{
    chart_sha256: string
    ln_policy: string
    double_option: string
    rule_mode: string
    scoring: string
  }>,
): BestScoreKey[] {
  const keys = new Map<string, BestScoreKey>()
  for (const row of rows) {
    const key: BestScoreKey = {
      chartSha256: row.chart_sha256,
      lnPolicy: row.ln_policy as LnScorePolicy,
      doubleOption: row.double_option as IrDoubleOption,
      ruleMode: row.rule_mode as IrRuleMode,
      scoring: row.scoring as 'bms_ex_score_v1',
    }
    keys.set(
      [key.chartSha256, key.lnPolicy, key.doubleOption, key.ruleMode, key.scoring].join('\u0000'),
      key,
    )
  }
  return [...keys.values()]
}

export function bestScoreKeyCondition(key: BestScoreKey) {
  return and(
    eq(schema.bestScores.chartSha256, key.chartSha256),
    eq(schema.bestScores.lnPolicy, key.lnPolicy),
    eq(schema.bestScores.doubleOption, key.doubleOption),
    eq(schema.bestScores.ruleMode, key.ruleMode),
    eq(schema.bestScores.scoring, key.scoring),
  )
}

export function scoreHistoryKeyCondition(key: BestScoreKey) {
  return and(
    eq(schema.scores.chartSha256, key.chartSha256),
    eq(schema.scores.lnPolicy, key.lnPolicy),
    eq(schema.scores.doubleOption, key.doubleOption),
    eq(schema.scores.ruleMode, key.ruleMode),
    eq(schema.scores.scoring, key.scoring),
  )
}

export async function findIdempotentScore(playerId: string, idempotencyKey: string) {
  return db.query.scores.findFirst({
    where: and(
      eq(schema.scores.playerId, playerId),
      eq(schema.scores.idempotencyKey, idempotencyKey),
    ),
  })
}

export function idempotentScoreResponse(
  score: {
    id: string
    serverReceivedAt: Date
  },
  rankings?: IrSubmitResponse['rankings'],
): IrSubmitResponse {
  return {
    accepted: true,
    score_id: score.id,
    best_updated: false,
    updated_fields: {
      ex_score: false,
      clear: false,
      max_combo: false,
      min_bp: false,
      min_cb: false,
    },
    server_received_at: score.serverReceivedAt.toISOString(),
    rankings: rankings && Object.keys(rankings).length > 0 ? rankings : undefined,
  }
}

export async function upsertChart(payload: IrScoreSubmission, allowUpdate: boolean) {
  const profile: Partial<IrChartLnProfile> = payload.chart.ln_profile ?? {}
  const notes = payload.chart.notes ?? {}
  const features = payload.chart.features ?? {}
  const values = {
    sha256: payload.chart.sha256,
    md5: payload.chart.md5 ?? null,
    title: payload.chart.title ?? '',
    subtitle: payload.chart.subtitle ?? null,
    genre: payload.chart.genre ?? null,
    artist: payload.chart.artist ?? null,
    subartists: payload.chart.subartists ?? [],
    mode: payload.chart.mode ?? payload.rule.key_mode ?? 'unknown',
    level: payload.chart.level ?? null,
    difficulty: payload.chart.difficulty ?? null,
    total: payload.chart.total ?? null,
    judgeRank: payload.chart.judge ?? null,
    minBpm: payload.chart.bpm?.min ?? null,
    maxBpm: payload.chart.bpm?.max ?? null,
    notes: notes.total ?? payload.result.notes,
    lnNotes: notes.ln ?? 0,
    cnNotes: notes.cn ?? 0,
    hcnNotes: notes.hcn ?? 0,
    mineNotes: notes.mine ?? 0,
    hasRandom: features.random ?? false,
    hasStop: features.stop ?? false,
    hasUndefinedLn: profile.has_undefined_ln ?? false,
    hasDefinedLn: profile.has_defined_ln ?? false,
    hasDefinedCn: profile.has_defined_cn ?? false,
    hasDefinedHcn: profile.has_defined_hcn ?? false,
    hasLn: features.ln ?? profile.has_defined_ln ?? profile.has_undefined_ln ?? false,
    hasCn: features.cn ?? profile.has_defined_cn ?? false,
    hasHcn: features.hcn ?? profile.has_defined_hcn ?? false,
    hasMine: features.mine ?? false,
    sourceUrl: payload.chart.urls?.source ?? null,
    appendUrl: payload.chart.urls?.append ?? null,
    headers: {},
    updatedAt: new Date(),
  }

  if (!allowUpdate) {
    await db.insert(schema.charts).values(values).onConflictDoNothing()
    return
  }
  await db
    .insert(schema.charts)
    .values(values)
    .onConflictDoUpdate({ target: schema.charts.sha256, set: values })
}

export function shouldUpdateExistingChart(
  playOptions: IrScoreSubmission['play_options'],
  doubleOption: IrDoubleOption,
): boolean {
  return doubleOption === 'off' && playOptions.submission_source !== 'local_backfill'
}

export async function fetchPreviousBest(
  playerId: string,
  payload: IrScoreSubmission,
): Promise<IrSubmitResponse['previous_best']> {
  const current = await db.query.bestScores.findFirst({
    columns: { exScore: true, clearType: true, maxCombo: true, minBp: true, minCb: true },
    where: and(
      eq(schema.bestScores.playerId, playerId),
      eq(schema.bestScores.chartSha256, payload.chart.sha256),
      eq(schema.bestScores.lnPolicy, payload.rule.ln_policy),
      eq(schema.bestScores.scoring, payload.rule.scoring),
      eq(schema.bestScores.doubleOption, normalizeDoubleOption(payload.play_options.double_option)),
      eq(schema.bestScores.ruleMode, payload.rule.rule_mode),
    ),
  })
  if (!current) {
    return null
  }
  return {
    clear_type: current.clearType,
    ex_score: current.exScore,
    max_combo: current.maxCombo,
    min_bp: current.minBp,
    min_cb: current.minCb,
  }
}

export async function fetchPreviousBestExScoreExcluding(
  playerId: string,
  payload: IrScoreSubmission,
  excludedScoreId: string,
): Promise<number | null> {
  const rows = await db
    .select({ ex_score: schema.scores.exScore })
    .from(schema.scores)
    .where(
      and(
        eq(schema.scores.playerId, playerId),
        eq(schema.scores.chartSha256, payload.chart.sha256),
        eq(schema.scores.lnPolicy, payload.rule.ln_policy),
        eq(schema.scores.scoring, payload.rule.scoring),
        eq(schema.scores.doubleOption, normalizeDoubleOption(payload.play_options.double_option)),
        eq(schema.scores.ruleMode, payload.rule.rule_mode),
        eq(schema.scores.accepted, true),
        ne(schema.scores.id, excludedScoreId),
      ),
    )
    .orderBy(desc(schema.scores.exScore))
    .limit(1)
  return rows[0]?.ex_score ?? null
}

/**
 * best_scores 更新の要否を判定し、必要なら未実行の upsert statement を返す。
 * 呼び出し側が score insert と同じ `db.batch` に載せて atomic に実行する。
 */
export async function prepareBestScoreUpsert(
  playerId: string,
  payload: IrScoreSubmission,
  scoreId: string,
  verification: string,
  candidate: BestScoreCandidate,
): Promise<{
  bestUpdated: boolean
  updatedFields: IrSubmitResponse['updated_fields']
  statement: BatchItem<'sqlite'> | null
}> {
  const current = await db.query.bestScores.findFirst({
    columns: {
      scoreId: true,
      exScore: true,
      clearType: true,
      clearRank: true,
      maxCombo: true,
      minBp: true,
      minCb: true,
      deviceType: true,
      gauge: true,
      effectiveLnMode: true,
      playedAt: true,
      serverReceivedAt: true,
      verification: true,
      bestExScoreId: true,
      bestClearScoreId: true,
      bestMaxComboScoreId: true,
      bestMinBpScoreId: true,
      bestMinCbScoreId: true,
    },
    where: and(
      eq(schema.bestScores.playerId, playerId),
      eq(schema.bestScores.chartSha256, payload.chart.sha256),
      eq(schema.bestScores.lnPolicy, payload.rule.ln_policy),
      eq(schema.bestScores.scoring, payload.rule.scoring),
      eq(schema.bestScores.doubleOption, normalizeDoubleOption(payload.play_options.double_option)),
      eq(schema.bestScores.ruleMode, payload.rule.rule_mode),
    ),
  })
  const currentCandidate = current
    ? {
        ex_score: current.exScore,
        clear_rank: current.clearRank,
        max_combo: current.maxCombo,
        min_bp: current.minBp,
        min_cb: current.minCb,
        server_received_at: current.serverReceivedAt,
      }
    : null

  const updatedFields = {
    ex_score: !currentCandidate || candidate.ex_score > currentCandidate.ex_score,
    clear: !currentCandidate || candidate.clear_rank > currentCandidate.clear_rank,
    max_combo: !currentCandidate || candidate.max_combo > currentCandidate.max_combo,
    min_bp: !currentCandidate || candidate.min_bp < currentCandidate.min_bp,
    min_cb: !currentCandidate || candidate.min_cb < currentCandidate.min_cb,
  }
  const rankingUpdated = !currentCandidate || bestCandidateWins(candidate, currentCandidate)
  const shouldUpdate =
    rankingUpdated ||
    updatedFields.clear ||
    updatedFields.max_combo ||
    updatedFields.min_bp ||
    updatedFields.min_cb
  if (!shouldUpdate) {
    return { bestUpdated: false, updatedFields, statement: null }
  }

  const verificationStatus = verification as IrVerificationStatus
  const playedAt = playedAtDate(payload.result.played_at)
  const values = {
    id: randomUUID(),
    playerId,
    chartSha256: payload.chart.sha256,
    scoreId,
    bestExScoreId: scoreId,
    bestClearScoreId: scoreId,
    bestMaxComboScoreId: scoreId,
    bestMinBpScoreId: scoreId,
    bestMinCbScoreId: scoreId,
    exScore: candidate.ex_score,
    clearType: payload.result.clear,
    clearRank: candidate.clear_rank,
    maxCombo: candidate.max_combo,
    minBp: candidate.min_bp,
    minCb: candidate.min_cb,
    deviceType: payload.play_options.device_type,
    doubleOption: normalizeDoubleOption(payload.play_options.double_option),
    gauge: payload.rule.gauge,
    lnPolicy: payload.rule.ln_policy,
    effectiveLnMode: payload.rule.effective_ln_mode,
    ruleMode: payload.rule.rule_mode,
    scoring: payload.rule.scoring,
    playedAt,
    serverReceivedAt: candidate.server_received_at,
    verification: verificationStatus,
  }
  // Compare with the row at write time, not the snapshot read above. Both values
  // and provenance IDs must advance together when concurrent submissions arrive.
  const table = schema.bestScores
  const rankingWins = sql`(excluded.ex_score > ${table.exScore} or
    (excluded.ex_score = ${table.exScore} and (excluded.clear_rank > ${table.clearRank} or
    (excluded.clear_rank = ${table.clearRank} and (excluded.min_bp < ${table.minBp} or
    (excluded.min_bp = ${table.minBp} and (excluded.min_cb < ${table.minCb} or
    (excluded.min_cb = ${table.minCb} and excluded.max_combo > ${table.maxCombo}))))))))`
  const clearWins = sql`excluded.clear_rank > ${table.clearRank}`
  const comboWins = sql`excluded.max_combo > ${table.maxCombo}`
  const bpWins = sql`excluded.min_bp < ${table.minBp}`
  const cbWins = sql`excluded.min_cb < ${table.minCb}`
  const choose = (condition: SQL, column: AnySQLiteColumn) =>
    sql`case when ${condition} then excluded.${sql.identifier(column.name)} else ${column} end`
  const statement = db
    .insert(schema.bestScores)
    .values(values)
    .onConflictDoUpdate({
      target: [
        schema.bestScores.playerId,
        schema.bestScores.chartSha256,
        schema.bestScores.lnPolicy,
        schema.bestScores.scoring,
        schema.bestScores.doubleOption,
        schema.bestScores.ruleMode,
      ],
      set: {
        scoreId: choose(rankingWins, table.scoreId),
        bestExScoreId: choose(rankingWins, table.bestExScoreId),
        bestClearScoreId: choose(clearWins, table.bestClearScoreId),
        bestMaxComboScoreId: choose(comboWins, table.bestMaxComboScoreId),
        bestMinBpScoreId: choose(bpWins, table.bestMinBpScoreId),
        bestMinCbScoreId: choose(cbWins, table.bestMinCbScoreId),
        exScore: choose(rankingWins, table.exScore),
        clearType: choose(clearWins, table.clearType),
        clearRank: choose(clearWins, table.clearRank),
        maxCombo: choose(comboWins, table.maxCombo),
        minBp: choose(bpWins, table.minBp),
        minCb: choose(cbWins, table.minCb),
        deviceType: choose(rankingWins, table.deviceType),
        gauge: choose(rankingWins, table.gauge),
        effectiveLnMode: choose(rankingWins, table.effectiveLnMode),
        playedAt: choose(rankingWins, table.playedAt),
        serverReceivedAt: choose(rankingWins, table.serverReceivedAt),
        verification: choose(rankingWins, table.verification),
        updatedAt: new Date(),
      },
    })
  return { bestUpdated: true, updatedFields, statement }
}
