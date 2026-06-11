import { serverSupabaseServiceRole } from '#supabase/server'
import type { Database } from '../../../../../../shared/types/database.types'

const REPLAY_BUCKET = 'replays'
const DOWNLOAD_URL_TTL_SECONDS = 300

/**
 * 検証済みリプレイの署名付きダウンロード URL を返す。
 * リプレイはランキングと同様に公開情報として扱う。
 */
export default defineEventHandler(async (event) => {
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: replay, error } = await db
    .from('replay_objects')
    .select('object_path, status, hash, format, size_bytes')
    .eq('score_id', scoreId)
    .maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  if (!replay || !replay.object_path || !['uploaded', 'verified'].includes(replay.status)) {
    throw createError({ statusCode: 404, statusMessage: 'Replay is not available' })
  }

  const { data: signed, error: signError } = await db.storage
    .from(REPLAY_BUCKET)
    .createSignedUrl(replay.object_path, DOWNLOAD_URL_TTL_SECONDS)
  if (signError || !signed) {
    throw createError({ statusCode: 500, statusMessage: signError?.message ?? 'sign failed' })
  }

  return {
    download_url: signed.signedUrl,
    expires_in: DOWNLOAD_URL_TTL_SECONDS,
    hash: replay.hash,
    format: replay.format,
    size_bytes: replay.size_bytes,
    status: replay.status,
  }
})
