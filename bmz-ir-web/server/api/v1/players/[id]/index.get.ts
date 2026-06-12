import { desc, eq, inArray } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'

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
    .where(eq(schema.profiles.id, playerId))
    .limit(1)
  const profile = profiles[0]
  if (!profile) {
    throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  }

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
      scoring: schema.bestScores.scoring,
      played_at: schema.bestScores.playedAt,
      server_received_at: schema.bestScores.serverReceivedAt,
    })
    .from(schema.bestScores)
    .where(eq(schema.bestScores.playerId, playerId))
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
          })
          .from(schema.charts)
          .where(inArray(schema.charts.sha256, shaList))
      : []
  const chartMap = new Map(charts.map((chart) => [chart.sha256, chart]))

  return {
    player: profile,
    best_scores: bests.map((row) => ({
      ...row,
      chart: chartMap.get(row.chart_sha256) ?? null,
    })),
  }
})
