import { and, eq } from 'drizzle-orm'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../../../utils/auth'
import { checkUserRateLimit } from '../../../../../utils/rate_limit'
import { MAX_REPLAY_BYTES } from '../../../../../../shared/constants/ir'

/**
 * リプレイ本体のアップロード。
 *
 * score_id はランキング API 経由で公開されるため、認証と replay object の
 * 所有者チェックがないと第三者が先にアップロードして verify を失敗させる
 * 妨害や、blob への任意書き込みが可能になる。upload-url 発行側と同じく
 * 本人のみ許可する。
 */
export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  await checkUserRateLimit(event, 'replay_upload', user.id, { user: 120, ip: 240 })
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  // body をメモリへ読む前に Content-Length で先に拒否する。
  const declaredLength = Number(getHeader(event, 'content-length') ?? 0)
  if (declaredLength > MAX_REPLAY_BYTES) {
    throw createError({ statusCode: 413, statusMessage: 'Replay body is too large' })
  }

  const replay = await db.query.replayObjects.findFirst({
    columns: { id: true, objectPath: true },
    where: and(
      eq(schema.replayObjects.scoreId, scoreId),
      eq(schema.replayObjects.playerId, user.id),
      eq(schema.replayObjects.status, 'pending_upload'),
    ),
  })
  if (!replay?.objectPath) {
    throw createError({ statusCode: 404, statusMessage: 'Replay upload intent not found' })
  }

  const body = await readRawBody(event, false)
  if (!body || body.length === 0) {
    throw createError({ statusCode: 400, statusMessage: 'Replay body is required' })
  }
  if (body.length > MAX_REPLAY_BYTES) {
    throw createError({ statusCode: 413, statusMessage: 'Replay body is too large' })
  }

  await blob.put(replay.objectPath, body, {
    contentType: 'application/octet-stream',
  })
  await db
    .update(schema.replayObjects)
    .set({ status: 'uploaded', sizeBytes: body.length, updatedAt: new Date() })
    .where(eq(schema.replayObjects.id, replay.id))

  return { uploaded: true, size_bytes: body.length }
})
