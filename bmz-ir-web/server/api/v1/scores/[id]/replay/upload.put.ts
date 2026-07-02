import { and, eq } from 'drizzle-orm'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../../../utils/auth'

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
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
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

  await blob.put(replay.objectPath, body, {
    contentType: 'application/octet-stream',
  })
  await db
    .update(schema.replayObjects)
    .set({ status: 'uploaded', sizeBytes: body.length, updatedAt: new Date() })
    .where(eq(schema.replayObjects.id, replay.id))

  return { uploaded: true, size_bytes: body.length }
})
