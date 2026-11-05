import { randomUUID } from 'node:crypto'
import { and, eq, isNull, sql } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { blob } from 'hub:blob'
import { requireScoreWriteAllowed } from '../../../../../services/moderation/enforcement'
import { requireIrUser } from '../../../../../utils/auth'
import { REPLAY_UPLOAD_RATE_LIMIT, checkUserRateLimit } from '../../../../../utils/rate_limit'

/**
 * リプレイアップロード用の PUT endpoint URL を発行する。
 *
 * - score は自分のもので、submit 時に replay hash を申告していること。
 * - replay_objects を pending_upload で upsert し、アップロード完了後は
 *   `POST /api/v1/scores/{id}/replay/verify` で hash 検証する。
 */
export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  await checkUserRateLimit(event, 'replay_upload', user.id, REPLAY_UPLOAD_RATE_LIMIT)
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }
  // 送信停止・BAN・非公開譜面のスコアにはリプレイを追加しない。
  await requireScoreWriteAllowed(user.id, scoreId)

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

  const previous = await db.query.replayObjects.findFirst({
    columns: { id: true, objectPath: true },
    where: eq(schema.replayObjects.scoreId, score.id),
  })
  const uploadId = randomUUID()
  const objectPath = `${user.id}/${score.id}/${uploadId}`
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
  const changed = await db
    .insert(schema.replayObjects)
    .values(values)
    .onConflictDoUpdate({
      target: schema.replayObjects.scoreId,
      set: {
        objectPath: values.objectPath,
        hash: values.hash,
        format: values.format,
        status: values.status,
        sizeBytes: null,
        updatedAt: values.updatedAt,
      },
      // Do not lose the path of a PUT that completed after the snapshot. It
      // must either be cleaned up here or fail its own conditional update.
      setWhere: previous
        ? and(
            eq(schema.replayObjects.id, previous.id),
            previous.objectPath === null
              ? isNull(schema.replayObjects.objectPath)
              : eq(schema.replayObjects.objectPath, previous.objectPath),
          )
        : sql`false`,
    })
    .returning({ id: schema.replayObjects.id })
  if (changed.length === 0) {
    throw createError({ statusCode: 409, statusMessage: 'Replay upload intent changed' })
  }

  if (previous?.objectPath) {
    // Every intent and PUT uses a new path, so cleanup cannot delete a newer upload.
    await blob.del(previous.objectPath).catch(() => {})
  }

  const uploadUrl = new URL(`/api/v1/scores/${score.id}/replay/upload`, getRequestURL(event))
  uploadUrl.searchParams.set('upload_id', uploadId)
  return {
    upload_url: uploadUrl.toString(),
    path: objectPath,
    required_hash: score.replayHash,
  }
})
