import { and, desc, eq, inArray } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
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
  const limit = Math.max(1, Math.min(200, Number(query.limit ?? 50) || 50))

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

  const bests = await db
    .select({
      score_id: schema.bestScores.scoreId,
      chart_sha256: schema.bestScores.chartSha256,
      ex_score: schema.bestScores.exScore,
      clear_type: schema.bestScores.clearType,
      clear_rank: schema.bestScores.clearRank,
      max_combo: schema.bestScores.maxCombo,
      min_bp: schema.bestScores.minBp,
      min_cb: schema.bestScores.minCb,
      device_type: schema.bestScores.deviceType,
      gauge: schema.bestScores.gauge,
      ln_policy: schema.bestScores.lnPolicy,
      rule_mode: schema.bestScores.ruleMode,
      scoring: schema.bestScores.scoring,
      played_at: schema.bestScores.playedAt,
      server_received_at: schema.bestScores.serverReceivedAt,
    })
    .from(schema.bestScores)
    .where(
      and(
        eq(schema.bestScores.playerId, playerId),
        publicChartCondition(schema.bestScores.chartSha256),
      ),
    )
    .orderBy(desc(schema.bestScores.serverReceivedAt))
    .limit(limit)

  const shaList = [...new Set(bests.map((row) => row.chart_sha256))]
  const charts =
    shaList.length > 0
      ? await db
          .select({
            sha256: schema.charts.sha256,
            title: schema.charts.title,
            artist: schema.charts.artist,
            mode: schema.charts.mode,
            level: schema.charts.level,
            difficulty: schema.charts.difficulty,
          })
          .from(schema.charts)
          .where(inArray(schema.charts.sha256, shaList))
      : []
  const chartMap = new Map(charts.map((chart) => [chart.sha256, chart]))

  return {
    player: profile,
    relationship: {
      is_self: isSelf,
      is_rival: outgoingRelationship.length > 0,
      is_rivaled_by: incomingRelationship.length > 0,
    },
    best_scores: bests.map((row) => ({
      ...row,
      chart: chartMap.get(row.chart_sha256) ?? null,
    })),
  }
})
