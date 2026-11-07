import { createHash } from 'node:crypto'
import { and, eq, inArray } from 'drizzle-orm'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'
import { requireScoreWriteAllowed } from '../../../../../services/moderation/enforcement'
import { requireIrUser } from '../../../../../utils/auth'
import { REPLAY_UPLOAD_RATE_LIMIT, checkUserRateLimit } from '../../../../../utils/rate_limit'
import { MAX_REPLAY_BYTES } from '../../../../../../shared/constants/ir'
import { readLimitedBody } from '../../../../../../shared/utils/body_limit'
import { deleteRejectedReplayBlobs } from '../../../../../services/storage_cleanup'

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
  // 送信停止・BAN・非公開譜面のスコアにはリプレイを追加しない。
  await requireScoreWriteAllowed(user.id, scoreId)

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
  if (replay.status === 'pending_upload') {
    throw createError({ statusCode: 409, statusMessage: 'Replay object is not uploaded yet' })
  }

  const stored = await blob.get(replay.objectPath)
  if (!stored) {
    throw createError({
      statusCode: 409,
      statusMessage: 'Replay object is not uploaded yet',
    })
  }

  const bytes = await readLimitedBody(stored.stream(), String(stored.size), MAX_REPLAY_BYTES)
  const actualHash = createHash('sha256').update(bytes).digest('hex')
  const verified = actualHash === replay.hash
  const status = verified ? 'verified' : 'rejected'

  const changed = await db
    .update(schema.replayObjects)
    .set({ status, sizeBytes: bytes.length, updatedAt: new Date() })
    .where(
      and(
        eq(schema.replayObjects.id, replay.id),
        eq(schema.replayObjects.objectPath, replay.objectPath),
        eq(schema.replayObjects.hash, replay.hash),
        inArray(schema.replayObjects.status, ['uploaded', 'verified']),
      ),
    )
    .returning({ id: schema.replayObjects.id })
  if (changed.length === 0) {
    throw createError({
      statusCode: 409,
      statusMessage: 'Replay upload changed during verification',
    })
  }

  if (!verified) {
    await deleteRejectedReplayBlobs([{ id: replay.id, objectPath: replay.objectPath }])
  }
  return { status, size_bytes: bytes.length, hash: actualHash }
})
