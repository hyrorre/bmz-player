import { createError, getRouterParam } from 'h3'
import { requireIrUser } from '../../../utils/auth'
import { revokeUserSessionById } from '../../../utils/auth_tokens'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const sessionId = getRouterParam(event, 'id') ?? ''
  if (!sessionId) {
    throw createError({ statusCode: 400, statusMessage: 'session id is required' })
  }

  const revoked = await revokeUserSessionById(user.id, sessionId)
  if (!revoked) {
    throw createError({ statusCode: 404, statusMessage: 'Session not found' })
  }

  return { revoked: true }
})
