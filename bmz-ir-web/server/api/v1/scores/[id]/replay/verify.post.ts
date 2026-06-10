import { createHash } from 'node:crypto'
import { serverSupabaseServiceRole } from '#supabase/server'
import { requireIrUser } from '../../../../../utils/auth'
import type { Database } from '../../../../../../shared/types/database.types'

const REPLAY_BUCKET = 'replays'

/**
 * アップロード済みリプレイの hash を検証する。
 *
 * submit 時に申告された `scores.replay_hash` と storage 上の実体の SHA256 が
 * 一致すれば `replay_objects.status = verified`、不一致なら `rejected`。
 */
export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: replay, error } = await db
    .from('replay_objects')
    .select('id, object_path, hash, status')
    .eq('score_id', scoreId)
    .eq('player_id', user.id)
    .maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  if (!replay || !replay.object_path) {
    throw createError({ statusCode: 404, statusMessage: 'Replay upload not found' })
  }

  const { data: file, error: downloadError } = await db.storage
    .from(REPLAY_BUCKET)
    .download(replay.object_path)
  if (downloadError || !file) {
    throw createError({
      statusCode: 409,
      statusMessage: downloadError?.message ?? 'Replay object is not uploaded yet',
    })
  }

  const bytes = Buffer.from(await file.arrayBuffer())
  const actualHash = createHash('sha256').update(bytes).digest('hex')
  const verified = actualHash === replay.hash
  const status = verified ? 'verified' : 'rejected'

  const { error: updateError } = await db
    .from('replay_objects')
    .update({ status, size_bytes: bytes.length, updated_at: new Date().toISOString() })
    .eq('id', replay.id)
  if (updateError) {
    throw createError({ statusCode: 500, statusMessage: updateError.message })
  }

  return { status, size_bytes: bytes.length, hash: actualHash }
})
