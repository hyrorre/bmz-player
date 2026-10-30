import { eq } from 'drizzle-orm'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'
import { isScoreVisibleTo } from '../../../../../services/moderation/visibility'
import { resolveIrUser } from '../../../../../utils/auth'

export default defineEventHandler(async (event) => {
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }
  if (!(await isReplayVisible(event, scoreId))) {
    throw createError({ statusCode: 404, statusMessage: 'Replay is not available' })
  }

  const replay = await db.query.replayObjects.findFirst({
    columns: { objectPath: true, status: true, format: true },
    where: eq(schema.replayObjects.scoreId, scoreId),
  })
  if (!replay?.objectPath || !['uploaded', 'verified'].includes(replay.status)) {
    throw createError({ statusCode: 404, statusMessage: 'Replay is not available' })
  }

  const stored = await blob.get(replay.objectPath)
  if (!stored) {
    throw createError({ statusCode: 404, statusMessage: 'Replay object is not available' })
  }

  return new Response(stored, {
    headers: {
      'content-type': 'application/octet-stream',
      'x-bmz-replay-format': replay.format,
    },
  })
})

/** BAN済みプレイヤー・非公開譜面のリプレイは本人以外に公開しない。 */
async function isReplayVisible(event: Parameters<typeof resolveIrUser>[0], scoreId: string) {
  const score = await db.query.scores.findFirst({
    columns: { playerId: true, chartSha256: true },
    where: eq(schema.scores.id, scoreId),
  })
  if (!score) {
    return false
  }
  const viewer = await resolveIrUser(event)
  return isScoreVisibleTo(score, viewer?.id ?? null)
}
