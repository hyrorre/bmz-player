import { readBody, createError } from 'h3'
import { rotateRefreshToken } from '../../../utils/auth_tokens'
import { irProviderKeyForEvent } from '../../../utils/provider_key'
import { checkIpRateLimit } from '../../../utils/rate_limit'

interface RefreshBody {
  refresh_token?: string
}

export default defineEventHandler(async (event) => {
  // 認証前 endpoint のため IP のみで制限。正規クライアントの refresh は
  // トークン失効前の低頻度アクセスで、この上限には届かない。
  await checkIpRateLimit(event, 'refresh', 60)
  const body = (await readBody(event)) as RefreshBody
  if (!body?.refresh_token) {
    throw createError({ statusCode: 400, statusMessage: 'refresh_token is required' })
  }

  const refreshed = await rotateRefreshToken(body.refresh_token)
  if (!refreshed) {
    throw createError({ statusCode: 401, statusMessage: 'Invalid refresh token' })
  }
  if ('reuseDetected' in refreshed) {
    throw createError({ statusCode: 401, statusMessage: 'Invalid refresh token' })
  }

  return {
    provider_key: irProviderKeyForEvent(event),
    access_token: refreshed.tokens.accessToken,
    refresh_token: refreshed.tokens.refreshToken,
    expires_at: refreshed.tokens.accessExpiresAt,
    player: {
      id: refreshed.user.id,
      email: refreshed.user.email,
      display_name: refreshed.user.displayName,
    },
  }
})
