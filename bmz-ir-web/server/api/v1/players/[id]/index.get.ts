import { and, count, desc, eq, or, sql } from 'drizzle-orm'
import { alias } from 'drizzle-orm/sqlite-core'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { IR_PLAYER_SCORE_PAGE_SIZE } from '../../../../../shared/constants/ir'
import { lookupDifficultyLabels } from '../../../../services/difficulty_tables'
import { arrangeOptionsFromPlayOptions, parseRankingQuery } from '../../../../services/ir/common'
import {
  publicChartCondition,
  publicPlayerCondition,
} from '../../../../services/moderation/visibility'
import { resolveIrUser } from '../../../../utils/auth'

export default defineEventHandler(async (event) => {
  const playerId = getRouterParam(event, 'id')
  if (!playerId) {
    throw createError({ statusCode: 400, statusMessage: 'player id is required' })
  }

  const query = getQuery(event)
  let scoreQuery
  try {
    scoreQuery = parseRankingQuery({
      limit: Number(query.limit) || IR_PLAYER_SCORE_PAGE_SIZE,
      offset: query.offset,
      ln_policy: query.ln_policy === 'ALL' ? undefined : query.ln_policy,
      rule_mode: query.rule_mode,
    })
  } catch (error) {
    throw createError({ statusCode: 400, statusMessage: (error as Error).message })
  }
  const { limit, offset, lnPolicy, ruleMode } = scoreQuery

  const profiles = await db
    .select({
      id: schema.profiles.id,
      display_name: schema.profiles.displayName,
      bio: schema.profiles.bio,
    })
    .from(schema.profiles)
    .where(and(eq(schema.profiles.id, playerId), publicPlayerCondition(schema.profiles.id)))
    .limit(1)
  const profile = profiles[0]
  if (!profile) {
    throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  }

  const user = await resolveIrUser(event)
  const isSelf = user?.id === playerId
  const [outgoingRelationship, incomingRelationship] =
    user && !isSelf
      ? await Promise.all([
          db
            .select({ ownerPlayerId: schema.rivalRelationships.ownerPlayerId })
            .from(schema.rivalRelationships)
            .where(
              and(
                eq(schema.rivalRelationships.ownerPlayerId, user.id),
                eq(schema.rivalRelationships.targetPlayerId, playerId),
                eq(schema.rivalRelationships.relationType, 'rival'),
              ),
            )
            .limit(1),
          db
            .select({ ownerPlayerId: schema.rivalRelationships.ownerPlayerId })
            .from(schema.rivalRelationships)
            .where(
              and(
                eq(schema.rivalRelationships.ownerPlayerId, playerId),
                eq(schema.rivalRelationships.targetPlayerId, user.id),
                eq(schema.rivalRelationships.relationType, 'rival'),
              ),
            )
            .limit(1),
        ])
      : [[], []]

  const best = schema.bestScores
  const chart = schema.charts
  const publicBests = and(eq(best.playerId, playerId), publicChartCondition(best.chartSha256))
  const chartJoin = eq(chart.sha256, best.chartSha256)
  const hasDefinedLn = or(
    chart.hasDefinedLn,
    chart.hasDefinedCn,
    chart.hasDefinedHcn,
    chart.hasDefinedHln,
  )
  const hasMixedLn = and(chart.hasUndefinedLn, hasDefinedLn)
  const hasLn = or(
    hasDefinedLn,
    chart.hasUndefinedLn,
    chart.hasLn,
    chart.hasCn,
    chart.hasHcn,
    chart.hasHln,
    chart.lnNotes,
    chart.cnNotes,
    chart.hcnNotes,
    chart.hlnNotes,
  )
  const [lnFilters] = await db
    .select({
      has_ln: sql`max(${hasLn})`.mapWith(Boolean),
      has_mixed_ln: sql`max(${hasMixedLn})`.mapWith(Boolean),
    })
    .from(best)
    .leftJoin(chart, chartJoin)
    .where(publicBests)
  const autoLn = lnPolicy?.startsWith('Auto')
  const filterLn = lnPolicy && (autoLn ? lnFilters?.has_mixed_ln : lnFilters?.has_ln)
  const where = and(
    publicBests,
    filterLn ? and(eq(best.lnPolicy, lnPolicy), autoLn ? hasMixedLn : hasLn) : undefined,
    ruleMode ? eq(best.ruleMode, ruleMode) : undefined,
  )

  const representative = alias(schema.scores, 'representative')
  const exBest = alias(schema.scores, 'ex_best')
  const comboBest = alias(schema.scores, 'combo_best')
  const [bests, counts] = await Promise.all([
    db
      .select({
        id: best.id,
        score_id: best.scoreId,
        best_ex_score_id: best.bestExScoreId,
        best_clear_score_id: best.bestClearScoreId,
        chart_sha256: best.chartSha256,
        ex_score: best.exScore,
        clear_type: best.clearType,
        clear_rank: best.clearRank,
        max_combo: best.maxCombo,
        min_bp: best.minBp,
        min_cb: best.minCb,
        device_type: best.deviceType,
        gauge: best.gauge,
        ln_policy: best.lnPolicy,
        rule_mode: best.ruleMode,
        scoring: best.scoring,
        played_at: best.playedAt,
        server_received_at: best.serverReceivedAt,
        playOptions: representative.playOptions,
        scoredNotes: exBest.notes,
        sourceCombo: comboBest.maxCombo,
        comboNotes: comboBest.notes,
        hasLn: sql`${hasLn}`.mapWith(Boolean),
        chart: {
          sha256: chart.sha256,
          md5: chart.md5,
          title: chart.title,
          subtitle: chart.subtitle,
          artist: chart.artist,
          mode: chart.mode,
          level: chart.level,
          difficulty: chart.difficulty,
        },
      })
      .from(best)
      .leftJoin(chart, chartJoin)
      .leftJoin(representative, eq(representative.id, best.scoreId))
      .leftJoin(exBest, eq(exBest.id, sql`coalesce(${best.bestExScoreId}, ${best.scoreId})`))
      .leftJoin(
        comboBest,
        eq(comboBest.id, sql`coalesce(${best.bestMaxComboScoreId}, ${best.scoreId})`),
      )
      .where(where)
      .orderBy(desc(best.serverReceivedAt), desc(best.id))
      .limit(limit)
      .offset(offset),
    db.select({ total: count() }).from(best).leftJoin(chart, chartJoin).where(where),
  ])
  const total = counts[0]?.total ?? 0
  const charts = new Map(bests.flatMap(({ chart }) => (chart ? [[chart.sha256, chart]] : [])))
  const difficultyLabels = await lookupDifficultyLabels([...charts.values()])

  return {
    player: profile,
    relationship: {
      is_self: isSelf,
      is_rival: outgoingRelationship.length > 0,
      is_rivaled_by: incomingRelationship.length > 0,
    },
    ln_filters: {
      has_ln: lnFilters?.has_ln ?? false,
      has_mixed_ln: lnFilters?.has_mixed_ln ?? false,
    },
    pagination: { limit, offset, total, has_more: offset + limit < total },
    best_scores: bests.map(
      ({ chart, hasLn, playOptions, scoredNotes, sourceCombo, comboNotes, ...score }) => ({
        ...score,
        ...arrangeOptionsFromPlayOptions(playOptions),
        max_ex_score: scoredNotes ? scoredNotes * 2 : null,
        max_combo_notes: sourceCombo === score.max_combo ? comboNotes : null,
        chart: chart && {
          ...chart,
          has_ln: Boolean(hasLn),
          difficulty_labels: difficultyLabels.get(chart.sha256) ?? [],
        },
      }),
    ),
  }
})
