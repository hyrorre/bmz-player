import { and, eq, sql } from 'drizzle-orm'
import { alias } from 'drizzle-orm/sqlite-core'
import { db, schema } from 'hub:db'
import { isUniqueConstraintError } from '../../utils/db_errors'
import { publicChartCondition, publicPlayerCondition } from '../moderation/visibility'
import type {
  IrAppliedDoubleOption,
  IrChartLnProfile,
  IrDeviceType,
  IrDoubleOption,
  IrJudgeCounts,
  IrJudges,
  IrRanking,
  IrRankingEntry,
  IrRuleMode,
  IrScoreSubmission,
  IrScoreSourceKind,
  IrSubmitResponse,
  IrVerificationStatus,
  LnScorePolicy,
} from '../../../shared/types/ir'

import {
  CLEAR_RANK,
  LOCAL_BACKFILL_SOURCE,
  type BestScoreCandidate,
  type BestScoreKey,
  type BestScoreRow,
  IrBackfillCleanupError,
  IrEvidenceValidationError,
  IrScoreNotFoundError,
  type IrRequestUser,
  type LocalBackfillDeleteResult,
  type RankingQuery,
  type ScoreAttestationPayload,
  type ScoreHistoryRankingRow,
  arrangeOptionsFromPlayOptions,
  bestCandidateWins,
  isRecord,
  judgeTotal,
  nonEmptyString,
  normalizeDoubleOption,
  normalizeGaugeName,
  playedAtDate,
  requireHex,
  scoreSubmissionMetadata,
} from './common'

export async function getRanking(
  user: IrRequestUser | null,
  sha256: string,
  query: RankingQuery,
): Promise<IrRanking> {
  return (await buildRanking(user, sha256, query)).data
}

export async function getRankingWithPreviousRank(
  user: IrRequestUser,
  sha256: string,
  query: RankingQuery,
  previousBestExScore: number | null,
): Promise<{ data: IrRanking; previousRank: number }> {
  return buildRanking(user, sha256, query, previousBestExScore)
}

async function buildRanking(
  user: IrRequestUser | null,
  sha256: string,
  query: RankingQuery,
  previousBestExScore: number | null = null,
): Promise<{ data: IrRanking; previousRank: number }> {
  requireHex(sha256, 64, 'sha256')
  const selfId = user?.id ?? null
  const rows = await fetchRankingPage(sha256, query, selfId, previousBestExScore)
  const summary = rows[0]!
  const page: IrRankingEntry[] = []
  let selfEntry: IrRankingEntry | undefined
  let selfInPage = false
  for (const row of rows) {
    if (row.player_id === null) continue
    const best = rowToBestScoreRow({
      ...row,
      played_at: row.played_at === null ? null : new Date(row.played_at),
      server_received_at: new Date(row.server_received_at),
    })
    const entry = rankingEntry(
      best,
      row.global_rank,
      row.global_pos,
      row.player_id === selfId,
      !!row.is_rival,
    )
    if (entry.relation.is_self) selfEntry = entry
    if (
      row.scope_pos !== null &&
      row.scope_pos > query.offset &&
      row.scope_pos <= query.offset + query.limit
    ) {
      // Self retains its global position even when the page uses a narrower scope.
      page.push({ ...entry, scope_rank: row.scope_pos })
      if (entry.relation.is_self) selfInPage = true
    }
  }
  await loadRankingEntryDetails(selfEntry && !selfInPage ? [...page, selfEntry] : page)
  return {
    previousRank: summary.previous_rank,
    data: {
      chart: { sha256 },
      rule: {
        scoring: query.scoring,
        ln_policy: query.lnPolicy,
        effective_ln_mode: query.lnPolicy ? (summary.rule_mode_ln ?? undefined) : undefined,
        double_option: query.doubleOption,
        rule_mode: query.ruleMode,
      },
      ranking: {
        scope: query.scope,
        sort: 'ex_score_desc',
        clear_rate: summary.clear_rate,
        entries: page,
        self: selfEntry
          ? {
              rank: selfEntry.rank,
              score_id: selfEntry.score.score_id,
              included_in_entries: selfInPage,
              entry: selfEntry,
            }
          : undefined,
        pagination: {
          limit: query.limit,
          offset: query.offset,
          total: summary.total,
          has_more: query.offset + query.limit < summary.total,
        },
      },
    },
  }
}

function rankingSource(
  table: typeof schema.bestScores | typeof schema.scores,
  sha256: string,
  query: RankingQuery,
) {
  const historical = table === schema.scores
  return sql`select player_id, chart_sha256, ex_score, clear_type, clear_rank, max_combo, min_bp, min_cb,
    device_type, gauge, ln_policy, effective_ln_mode, double_option, rule_mode, played_at, server_received_at, verification,
    ${
      historical
        ? sql`id as score_id, id as best_ex_score_id, id as best_clear_score_id,
      id as best_max_combo_score_id, id as best_min_bp_score_id, id as best_min_cb_score_id`
        : sql`score_id, best_ex_score_id, best_clear_score_id, best_max_combo_score_id, best_min_bp_score_id, best_min_cb_score_id`
    }
    from ${table} where ${and(
      eq(table.chartSha256, sha256),
      eq(table.scoring, query.scoring),
      eq(table.doubleOption, query.doubleOption),
      query.lnPolicy ? eq(table.lnPolicy, query.lnPolicy) : undefined,
      query.ruleMode ? eq(table.ruleMode, query.ruleMode) : undefined,
      historical ? sql`accepted = 1 and not exists (select 1 from best)` : undefined,
      publicPlayerCondition(table.playerId),
      publicChartCondition(table.chartSha256),
    )}`
}

function fetchRankingPage(
  sha256: string,
  query: RankingQuery,
  selfId: string | null,
  previous: number | null,
) {
  const self = sql`player_id = ${selfId}`
  const around = sql`(select global_pos from ranked where ${self})`
  const scopes = {
    global: sql`1`,
    self,
    rivals: sql`is_rival`,
    self_and_rivals: sql`(${self} or is_rival)`,
    around_self: sql`(${around} is null or abs(global_pos - ${around}) <= 5)`,
  }
  // Rank globally, then filter the scope. Fold source IDs only for the selected players.
  type PageRow = Omit<
    Parameters<typeof rowToBestScoreRow>[0],
    'played_at' | 'server_received_at'
  > & {
    played_at: number | null
    server_received_at: number
    global_rank: number
    global_pos: number
    scope_pos: number | null
    is_rival: number
    clear_rate: number | null
    total: number
    previous_rank: number
    rule_mode_ln: BestScoreRow['effective_ln_mode'] | null
  }
  return db.all<PageRow>(sql`
    with best as (${rankingSource(schema.bestScores, sha256, query)}),
    source as (select * from best union all ${rankingSource(schema.scores, sha256, query)}),
    representatives as (
      select *, row_number() over (partition by player_id order by ex_score desc, clear_rank desc,
        min_bp, min_cb, max_combo desc, coalesce(played_at,server_received_at), score_id) as representative from source
    ), players as (
      select player_id, max(ex_score) as ex_score, max(clear_rank) as clear_rank, min(min_bp) as bp, min(min_cb) as cb,
        max(max_combo) as combo, max(case when representative=1 then coalesce(played_at,server_received_at) end) as played
      from representatives group by player_id
    ), ranked as (
      select *, rank() over(order by ex_score desc) as global_rank,
        row_number() over(order by ex_score desc, clear_rank desc, bp, cb, combo desc, played, player_id) as global_pos,
        player_id in (select target_player_id from rival_relationships where owner_player_id=${selfId} and relation_type='rival') as is_rival
      from players
    ), scoped as (
      select player_id, row_number() over(order by global_pos) as scope_pos from ranked where ${scopes[query.scope]}
    ), visible as (
      select ranked.*, scoped.scope_pos from ranked left join scoped using(player_id)
      where (scope_pos > ${query.offset} and scope_pos <= ${query.offset + query.limit}) or ranked.player_id=${selfId}
    ), winners as (
      select r.player_id, r.chart_sha256, r.score_id, r.best_ex_score_id, r.ex_score, r.gauge, r.ln_policy,
        r.effective_ln_mode, r.rule_mode, r.double_option, r.device_type, r.played_at, r.server_received_at, r.verification,
        v.global_rank, v.global_pos, v.is_rival, v.scope_pos, v.clear_rank, v.bp as min_bp, v.cb as min_cb, v.combo as max_combo,
        r.representative, first_value(r.clear_type) over clearest as clear_type,
        first_value(coalesce(r.best_clear_score_id,r.score_id)) over clearest as best_clear_score_id,
        first_value(coalesce(r.best_max_combo_score_id,r.score_id)) over combo as best_max_combo_score_id,
        first_value(coalesce(r.best_min_bp_score_id,r.score_id)) over bp as best_min_bp_score_id,
        first_value(coalesce(r.best_min_cb_score_id,r.score_id)) over cb as best_min_cb_score_id
      from visible v join representatives r using(player_id)
      window clearest as (partition by r.player_id order by r.clear_rank desc, coalesce(r.played_at,r.server_received_at), r.ex_score desc, r.score_id),
        combo as (partition by r.player_id order by r.max_combo desc, r.ex_score desc, r.score_id),
        bp as (partition by r.player_id order by r.min_bp, r.ex_score desc, r.score_id),
        cb as (partition by r.player_id order by r.min_cb, r.ex_score desc, r.score_id)
    ), summary as (
      select (select count(*) from scoped) as total, round(100.0 * avg(clear_rank>1)) as clear_rate,
        case when ${previous} is null then 0 else 1+count(case when ex_score>${previous} and player_id!=${selfId} then 1 end) end as previous_rank,
        (select effective_ln_mode from source order by ex_score desc limit 1) as rule_mode_ln from players
    )
    select summary.*, winners.* from summary left join winners on winners.representative=1 order by winners.global_pos
  `)
}

async function loadRankingEntryDetails(entries: IrRankingEntry[]): Promise<void> {
  if (entries.length === 0) return

  // A 200-entry page plus the player's own entry must fit D1's 100-bind limit.
  const sources = JSON.stringify(
    entries.map(({ player, score }) => ({
      playerId: player.id,
      scoreId: score.score_id,
      exScoreId: score.source_score_ids?.ex_score ?? score.score_id,
      comboScoreId: score.source_score_ids?.max_combo ?? score.score_id,
    })),
  )
  const playerId = sql<string>`json_extract(ranking_sources.value, '$.playerId')`
  const representative = alias(schema.scores, 'representative')
  const exBest = alias(schema.scores, 'ex_best')
  const comboBest = alias(schema.scores, 'combo_best')
  const rows = await db
    .select({
      playerId,
      displayName: schema.profiles.displayName,
      playOptions: representative.playOptions,
      judges: exBest.judges,
      notes: exBest.notes,
      combo: comboBest.maxCombo,
      comboNotes: comboBest.notes,
    })
    .from(sql`json_each(${sources}) as ranking_sources`)
    .leftJoin(schema.profiles, eq(schema.profiles.id, playerId))
    .leftJoin(
      representative,
      eq(representative.id, sql`json_extract(ranking_sources.value, '$.scoreId')`),
    )
    .leftJoin(exBest, eq(exBest.id, sql`json_extract(ranking_sources.value, '$.exScoreId')`))
    .leftJoin(
      comboBest,
      eq(comboBest.id, sql`json_extract(ranking_sources.value, '$.comboScoreId')`),
    )

  const byPlayer = new Map(rows.map((row) => [row.playerId, row]))
  for (const entry of entries) {
    const details = byPlayer.get(entry.player.id)
    entry.player.display_name = details?.displayName || 'Player'
    Object.assign(entry.score, arrangeOptionsFromPlayOptions(details?.playOptions), {
      judges: rankingJudges(details?.judges),
      max_ex_score: details?.notes ? details.notes * 2 : null,
      max_combo_notes: details?.combo === entry.score.max_combo ? details.comboNotes : null,
    })
  }
}

export function rowToBestScoreRow(row: {
  player_id: string
  chart_sha256: string
  score_id: string
  best_ex_score_id?: string | null
  best_clear_score_id?: string | null
  best_max_combo_score_id?: string | null
  best_min_bp_score_id?: string | null
  best_min_cb_score_id?: string | null
  ex_score: number
  clear_type: string
  clear_rank: number
  max_combo: number
  min_bp: number
  min_cb: number
  device_type: string
  gauge: string
  ln_policy: string
  effective_ln_mode: string
  double_option: string
  rule_mode: string
  played_at: Date | null
  server_received_at: Date
  verification: BestScoreRow['verification']
}): BestScoreRow {
  return {
    ...row,
    best_ex_score_id: row.best_ex_score_id ?? row.score_id,
    best_clear_score_id: row.best_clear_score_id ?? row.score_id,
    best_max_combo_score_id: row.best_max_combo_score_id ?? row.score_id,
    best_min_bp_score_id: row.best_min_bp_score_id ?? row.score_id,
    best_min_cb_score_id: row.best_min_cb_score_id ?? row.score_id,
    scoring: 'bms_ex_score_v1',
    ln_policy: row.ln_policy as LnScorePolicy,
    effective_ln_mode: row.effective_ln_mode as 'ln' | 'cn' | 'hcn' | 'hln',
    double_option: row.double_option as IrDoubleOption,
    rule_mode: row.rule_mode as IrRuleMode,
    device_type: row.device_type as IrDeviceType,
    played_at: row.played_at?.toISOString() ?? null,
  }
}

export function rankingJudgeCounts(value: unknown): IrJudgeCounts | undefined {
  if (!isRecord(value)) {
    return undefined
  }
  const keys = ['pgreat', 'great', 'good', 'bad', 'poor', 'empty_poor'] as const
  if (keys.some((key) => !Number.isInteger(value[key]) || Number(value[key]) < 0)) {
    return undefined
  }
  return {
    pgreat: Number(value.pgreat),
    great: Number(value.great),
    good: Number(value.good),
    bad: Number(value.bad),
    poor: Number(value.poor),
    empty_poor: Number(value.empty_poor),
  }
}

export function rankingJudges(value: unknown): IrJudges | undefined {
  if (!isRecord(value)) {
    return undefined
  }
  const fast = rankingJudgeCounts(value.fast)
  const slow = rankingJudgeCounts(value.slow)
  return fast && slow ? { fast, slow } : undefined
}

export function bestRowsFromHistory(rows: ScoreHistoryRankingRow[]): BestScoreRow[] {
  const bestByRule = new Map<string, BestScoreRow>()
  for (const row of rows) {
    const candidate = historyRowToBestRow(row)
    const key = bestRowKey(candidate)
    const current = bestByRule.get(key)
    if (current) {
      bestByRule.set(key, mergeBestRows(current, candidate))
    } else {
      bestByRule.set(key, candidate)
    }
  }
  return [...bestByRule.values()]
}

export function historyRowToBestRow(row: ScoreHistoryRankingRow): BestScoreRow {
  const { id, ...score } = row
  return { ...score, score_id: id }
}

export function bestRowKey(row: BestScoreRow): string {
  return [
    row.player_id,
    row.chart_sha256,
    row.ln_policy,
    row.scoring,
    row.double_option,
    row.rule_mode,
  ].join('\0')
}

export function bestRowWins(next: BestScoreRow, current: BestScoreRow): boolean {
  if (bestCandidateWins(next, current)) {
    return true
  }
  if (bestCandidateWins(current, next)) {
    return false
  }
  return (
    String(next.played_at ?? next.server_received_at).localeCompare(
      String(current.played_at ?? current.server_received_at),
    ) < 0
  )
}

export function mergeBestRows(current: BestScoreRow, next: BestScoreRow): BestScoreRow {
  const ranking = bestRowWins(next, current) ? next : current
  const clear = bestClearWins(next, current) ? next : current
  const combo = next.max_combo > current.max_combo ? next : current
  const bp = next.min_bp < current.min_bp ? next : current
  const cb = next.min_cb < current.min_cb ? next : current

  return {
    ...ranking,
    clear_type: clear.clear_type,
    clear_rank: clear.clear_rank,
    max_combo: combo.max_combo,
    min_bp: bp.min_bp,
    min_cb: cb.min_cb,
    best_ex_score_id: ranking.best_ex_score_id,
    best_clear_score_id: clear.best_clear_score_id,
    best_max_combo_score_id: combo.best_max_combo_score_id,
    best_min_bp_score_id: bp.best_min_bp_score_id,
    best_min_cb_score_id: cb.best_min_cb_score_id,
  }
}

export function bestClearWins(next: BestScoreRow, current: BestScoreRow): boolean {
  if (next.clear_rank !== current.clear_rank) {
    return next.clear_rank > current.clear_rank
  }
  return (
    String(next.played_at ?? next.server_received_at).localeCompare(
      String(current.played_at ?? current.server_received_at),
    ) < 0
  )
}

function rankingEntry(
  row: BestScoreRow,
  rank: number,
  position: number,
  isSelf: boolean,
  isRival: boolean,
): IrRankingEntry {
  return {
    rank,
    scope_rank: position,
    player: {
      id: row.player_id,
      display_name: 'Player',
    },
    score: {
      score_id: row.score_id,
      clear: row.clear_type,
      ex_score: row.ex_score,
      max_combo: row.max_combo,
      min_bp: row.min_bp,
      min_cb: row.min_cb,
      gauge: row.gauge,
      ln_policy: row.ln_policy,
      double_option: row.double_option,
      rule_mode: row.rule_mode,
      device_type: row.device_type,
      played_at: row.played_at,
      verification: row.verification,
      source_score_ids: {
        ex_score: row.best_ex_score_id,
        clear: row.best_clear_score_id,
        max_combo: row.best_max_combo_score_id,
        min_bp: row.best_min_bp_score_id,
        min_cb: row.best_min_cb_score_id,
      },
    },
    relation: {
      is_self: isSelf,
      is_rival: isRival,
    },
  }
}

/**
 * tamper evidence の署名を検証する。
 *
 * - evidence なし / 署名なし → unverified
 * - 署名ありで device key 不明・hash 不一致・署名不正 → reject
 * - 検証成功 → submission source に応じた verified_play / signed_backfill
 *
 * canonical form は「evidence を除いた payload をキー昇順 compact JSON 化」
 * したもので、BMZ クライアント (serde_json の BTreeMap 出力) と一致させる。
 */
