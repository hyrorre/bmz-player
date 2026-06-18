import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'

/**
 * スコア詳細。ランキングに公開されている情報の単票ビュー。
 * 署名・リプレイの検証状態も返す。
 */
export default defineEventHandler(async (event) => {
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const scores = await db
    .select({
      id: schema.scores.id,
      player_id: schema.scores.playerId,
      chart_sha256: schema.scores.chartSha256,
      clear_type: schema.scores.clearType,
      ex_score: schema.scores.exScore,
      max_combo: schema.scores.maxCombo,
      min_bp: schema.scores.minBp,
      min_cb: schema.scores.minCb,
      bp: schema.scores.bp,
      cb: schema.scores.cb,
      gauge: schema.scores.gauge,
      ln_policy: schema.scores.lnPolicy,
      effective_ln_mode: schema.scores.effectiveLnMode,
      rule_mode: schema.scores.ruleMode,
      scoring: schema.scores.scoring,
      judges: schema.scores.judges,
      device_type: schema.scores.deviceType,
      platform: schema.scores.platform,
      client_name: schema.scores.clientName,
      client_version: schema.scores.clientVersion,
      played_at: schema.scores.playedAt,
      server_received_at: schema.scores.serverReceivedAt,
      verification: schema.scores.verification,
      replay_hash: schema.scores.replayHash,
    })
    .from(schema.scores)
    .where(eq(schema.scores.id, scoreId))
    .limit(1)
  const score = scores[0]
  if (!score) {
    throw createError({ statusCode: 404, statusMessage: 'Score not found' })
  }

  const [profiles, charts, replays] = await Promise.all([
    db
      .select({ id: schema.profiles.id, display_name: schema.profiles.displayName })
      .from(schema.profiles)
      .where(eq(schema.profiles.id, score.player_id))
      .limit(1),
    db
      .select({
        sha256: schema.charts.sha256,
        title: schema.charts.title,
        subtitle: schema.charts.subtitle,
        artist: schema.charts.artist,
        mode: schema.charts.mode,
        level: schema.charts.level,
        notes: schema.charts.notes,
      })
      .from(schema.charts)
      .where(eq(schema.charts.sha256, score.chart_sha256))
      .limit(1),
    db
      .select({
        status: schema.replayObjects.status,
        size_bytes: schema.replayObjects.sizeBytes,
        format: schema.replayObjects.format,
      })
      .from(schema.replayObjects)
      .where(eq(schema.replayObjects.scoreId, score.id))
      .limit(1),
  ])

  return {
    score,
    player: profiles[0] ?? { id: score.player_id, display_name: 'Player' },
    chart: charts[0] ?? null,
    replay: replays[0] ?? null,
  }
})
