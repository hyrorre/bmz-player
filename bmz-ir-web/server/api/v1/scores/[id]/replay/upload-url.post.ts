import { randomUUID } from 'node:crypto'
import { and, eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../../../utils/auth'
import { checkUserRateLimit } from '../../../../../utils/rate_limit'

/**
 * リプレイアップロード用の PUT endpoint URL を発行する。
 *
 * - score は自分のもので、submit 時に replay hash を申告していること。
 * - replay_objects を pending_upload で upsert し、アップロード完了後は
 *   `POST /api/v1/scores/{id}/replay/verify` で hash 検証する。
 */
export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  await checkUserRateLimit(event, 'replay_upload', user.id, { user: 120, ip: 240 })
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const score = await db.query.scores.findFirst({
    columns: { id: true, playerId: true, replayHash: true, replayFormat: true },
    where: and(eq(schema.scores.id, scoreId), eq(schema.scores.playerId, user.id)),
  })
  if (!score) {
    throw createError({ statusCode: 404, statusMessage: 'Score not found' })
  }
  if (!score.replayHash) {
    throw createError({ statusCode: 409, statusMessage: 'Score has no declared replay hash' })
  }

  const objectPath = `${user.id}/${score.id}`
  const values = {
    id: randomUUID(),
    scoreId: score.id,
    playerId: user.id,
    objectPath,
    hash: score.replayHash,
    format: score.replayFormat ?? 'bmz-replay-v1',
    status: 'pending_upload' as const,
    updatedAt: new Date(),
  }
  await db
    .insert(schema.replayObjects)
    .values(values)
    .onConflictDoUpdate({
      target: schema.replayObjects.scoreId,
      set: {
        objectPath: values.objectPath,
        hash: values.hash,
        format: values.format,
        status: values.status,
        updatedAt: values.updatedAt,
      },
    })

  const uploadUrl = new URL(`/api/v1/scores/${score.id}/replay/upload`, getRequestURL(event))
  return {
    upload_url: uploadUrl.toString(),
    path: objectPath,
    required_hash: score.replayHash,
  }
})
