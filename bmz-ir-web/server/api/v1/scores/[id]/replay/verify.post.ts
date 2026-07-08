import { createHash } from 'node:crypto'
import { and, eq } from 'drizzle-orm'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../../../utils/auth'
import { REPLAY_UPLOAD_RATE_LIMIT, checkUserRateLimit } from '../../../../../utils/rate_limit'

/**
 * アップロード済みリプレイの hash を検証する。
 *
 * submit 時に申告された `scores.replay_hash` と storage 上の実体の SHA256 が
 * 一致すれば `replay_objects.status = verified`、不一致なら `rejected`。
 */
export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  // blob 全体の読み出し + hash 計算を伴うため upload と同じ枠で数える。
  await checkUserRateLimit(event, 'replay_upload', user.id, REPLAY_UPLOAD_RATE_LIMIT)
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const replay = await db.query.replayObjects.findFirst({
    columns: { id: true, objectPath: true, hash: true, status: true },
    where: and(
      eq(schema.replayObjects.scoreId, scoreId),
      eq(schema.replayObjects.playerId, user.id),
    ),
  })
  if (!replay || !replay.objectPath) {
    throw createError({ statusCode: 404, statusMessage: 'Replay upload not found' })
  }

  const stored = await blob.get(replay.objectPath)
  if (!stored) {
    throw createError({
      statusCode: 409,
      statusMessage: 'Replay object is not uploaded yet',
    })
  }

  const bytes = Buffer.from(await stored.arrayBuffer())
  const actualHash = createHash('sha256').update(bytes).digest('hex')
  const verified = actualHash === replay.hash
  const status = verified ? 'verified' : 'rejected'

  await db
    .update(schema.replayObjects)
    .set({ status, sizeBytes: bytes.length, updatedAt: new Date() })
    .where(eq(schema.replayObjects.id, replay.id))

  return { status, size_bytes: bytes.length, hash: actualHash }
})
