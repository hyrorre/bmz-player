import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { isScoreVisibleTo } from '../../../../../services/moderation/visibility'
import { resolveIrUser } from '../../../../../utils/auth'

const DOWNLOAD_URL_TTL_SECONDS = 300

/**
 * 検証済みリプレイのダウンロード URL を返す。
 * リプレイはランキングと同様に公開情報として扱う。
 */
export default defineEventHandler(async (event) => {
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }
  if (!(await isReplayVisible(event, scoreId))) {
    throw createError({ statusCode: 404, statusMessage: 'Replay is not available' })
  }

  const replay = await db.query.replayObjects.findFirst({
    columns: { objectPath: true, status: true, hash: true, format: true, sizeBytes: true },
    where: eq(schema.replayObjects.scoreId, scoreId),
  })
  if (!replay || !replay.objectPath || !['uploaded', 'verified'].includes(replay.status)) {
    throw createError({ statusCode: 404, statusMessage: 'Replay is not available' })
  }

  const downloadUrl = new URL(`/api/v1/scores/${scoreId}/replay/raw`, getRequestURL(event))

  return {
    download_url: downloadUrl.toString(),
    expires_in: DOWNLOAD_URL_TTL_SECONDS,
    hash: replay.hash,
    format: replay.format,
    size_bytes: replay.sizeBytes,
    status: replay.status,
  }
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
