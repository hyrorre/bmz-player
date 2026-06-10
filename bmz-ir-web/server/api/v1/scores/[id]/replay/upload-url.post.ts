import { serverSupabaseServiceRole } from '#supabase/server'
import { requireIrUser } from '../../../../../utils/auth'
import type { Database } from '../../../../../../shared/types/database.types'

const REPLAY_BUCKET = 'replays'

/**
 * リプレイアップロード用の署名付き URL を発行する。
 *
 * - score は自分のもので、submit 時に replay hash を申告していること。
 * - replay_objects を pending_upload で upsert し、アップロード完了後は
 *   `POST /api/v1/scores/{id}/replay/verify` で hash 検証する。
 */
export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: score, error } = await db
    .from('scores')
    .select('id, player_id, replay_hash, replay_format')
    .eq('id', scoreId)
    .eq('player_id', user.id)
    .maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  if (!score) {
    throw createError({ statusCode: 404, statusMessage: 'Score not found' })
  }
  if (!score.replay_hash) {
    throw createError({ statusCode: 409, statusMessage: 'Score has no declared replay hash' })
  }

  const objectPath = `${user.id}/${score.id}`
  const { error: upsertError } = await db.from('replay_objects').upsert(
    {
      score_id: score.id,
      player_id: user.id,
      object_path: objectPath,
      hash: score.replay_hash,
      format: score.replay_format ?? 'bmz-replay-v1',
      status: 'pending_upload',
      updated_at: new Date().toISOString(),
    },
    { onConflict: 'score_id' },
  )
  if (upsertError) {
    throw createError({ statusCode: 500, statusMessage: upsertError.message })
  }

  const { data: signed, error: signError } = await db.storage
    .from(REPLAY_BUCKET)
    .createSignedUploadUrl(objectPath, { upsert: true })
  if (signError || !signed) {
    throw createError({ statusCode: 500, statusMessage: signError?.message ?? 'sign failed' })
  }

  return {
    upload_url: signed.signedUrl,
    token: signed.token,
    path: signed.path,
    required_hash: score.replay_hash,
  }
})
