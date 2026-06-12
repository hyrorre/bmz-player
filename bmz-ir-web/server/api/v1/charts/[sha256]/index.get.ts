import { and, eq, sql } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { resolveIrUser } from '../../../../utils/auth'

export default defineEventHandler(async (event) => {
  const sha256 = getRouterParam(event, 'sha256')
  if (!sha256) {
    throw createError({ statusCode: 400, statusMessage: 'chart sha256 is required' })
  }

  const user = await resolveIrUser(event)
  const charts = await db
    .select()
    .from(schema.charts)
    .where(eq(schema.charts.sha256, sha256))
    .limit(1)
  const chart = charts[0]
  if (!chart) {
    throw createError({ statusCode: 404, statusMessage: 'Chart not found' })
  }

  const globalStats = await scoreStats(sha256)
  const selfStats = user ? await scoreStats(sha256, user.id) : null

  return {
    chart: chartToResponse(chart),
    stats: {
      global: globalStats,
      self: selfStats,
    },
  }
})

async function scoreStats(sha256: string, playerId?: string) {
  const filters = [eq(schema.scores.chartSha256, sha256), eq(schema.scores.accepted, true)]
  if (playerId) {
    filters.push(eq(schema.scores.playerId, playerId))
  }

  const rows = await db
    .select({
      play_count: sql<number>`count(*)`,
      clear_count: sql<number>`sum(case when ${schema.scores.clearRank} > 1 then 1 else 0 end)`,
    })
    .from(schema.scores)
    .where(and(...filters))

  return {
    play_count: Number(rows[0]?.play_count ?? 0),
    clear_count: Number(rows[0]?.clear_count ?? 0),
  }
}

function chartToResponse(chart: typeof schema.charts.$inferSelect) {
  return {
    sha256: chart.sha256,
    md5: chart.md5,
    title: chart.title,
    subtitle: chart.subtitle,
    genre: chart.genre,
    artist: chart.artist,
    subartists: chart.subartists,
    mode: chart.mode,
    level: chart.level,
    total: chart.total,
    judge_rank: chart.judgeRank,
    min_bpm: chart.minBpm,
    max_bpm: chart.maxBpm,
    notes: chart.notes,
    ln_notes: chart.lnNotes,
    cn_notes: chart.cnNotes,
    hcn_notes: chart.hcnNotes,
    mine_notes: chart.mineNotes,
    has_random: chart.hasRandom,
    has_stop: chart.hasStop,
    has_undefined_ln: chart.hasUndefinedLn,
    has_defined_ln: chart.hasDefinedLn,
    has_defined_cn: chart.hasDefinedCn,
    has_defined_hcn: chart.hasDefinedHcn,
    has_ln: chart.hasLn,
    has_cn: chart.hasCn,
    has_hcn: chart.hasHcn,
    has_mine: chart.hasMine,
    source_url: chart.sourceUrl,
    append_url: chart.appendUrl,
    headers: chart.headers,
    created_at: chart.createdAt,
    updated_at: chart.updatedAt,
  }
}
