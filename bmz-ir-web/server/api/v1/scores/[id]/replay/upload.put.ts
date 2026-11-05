import { and, eq } from 'drizzle-orm'
import { randomUUID } from 'node:crypto'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'
import { requireScoreWriteAllowed } from '../../../../../services/moderation/enforcement'
import { requireIrUser } from '../../../../../utils/auth'
import { REPLAY_UPLOAD_RATE_LIMIT, checkUserRateLimit } from '../../../../../utils/rate_limit'
import { readReplayBody } from '../../../../../utils/replay_body'

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
  await checkUserRateLimit(event, 'replay_upload', user.id, REPLAY_UPLOAD_RATE_LIMIT)
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }
  // 送信停止・BAN・非公開譜面のスコアにはリプレイを追加しない。
  await requireScoreWriteAllowed(user.id, scoreId)

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

  if (getQuery(event).upload_id !== replay.objectPath.split('/').at(-1)) {
    throw createError({ statusCode: 409, statusMessage: 'Replay upload intent changed' })
  }
  const body = await readReplayBody(event)
  if (body.length === 0) {
    throw createError({ statusCode: 400, statusMessage: 'Replay body is required' })
  }
  // Concurrent PUTs must never overwrite bytes that verification has already read.
  const objectPath = `${replay.objectPath}/${randomUUID()}`
  await blob.put(objectPath, body, {
    contentType: 'application/octet-stream',
  })
  const changed = await db
    .update(schema.replayObjects)
    .set({ objectPath, status: 'uploaded', sizeBytes: body.length, updatedAt: new Date() })
    .where(
      and(
        eq(schema.replayObjects.id, replay.id),
        eq(schema.replayObjects.objectPath, replay.objectPath),
        eq(schema.replayObjects.status, 'pending_upload'),
      ),
    )
    .returning({ id: schema.replayObjects.id })
  if (changed.length === 0) {
    await blob.del(objectPath).catch(() => {})
    throw createError({ statusCode: 409, statusMessage: 'Replay upload intent changed' })
  }

  return { uploaded: true, size_bytes: body.length }
})
