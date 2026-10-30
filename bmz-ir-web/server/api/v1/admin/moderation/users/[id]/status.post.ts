import { readBody } from 'h3'
import {
  changeUserModeration,
  parseExpectedRevision,
  parseReason,
  parseUserModerationStatus,
} from '../../../../../../services/moderation/admin'
import { handleModerationAdmin } from '../../../../../../services/moderation/http'
import { configuredAdminUserIds, requireIrAdmin } from '../../../../../../utils/admin'

interface UserStatusBody {
  status?: unknown
  reason?: unknown
  expected_revision?: unknown
}

/**
 * ユーザーの利用状態を変更する。操作者はセッションから決め、body の値は使わない。
 * expected_revision が現在値と異なる場合は 409 を返し、古い画面からの上書きを防ぐ。
 */
export default defineEventHandler(async (event) => {
  const admin = await requireIrAdmin(event)
  const userId = getRouterParam(event, 'id')?.trim()
  if (!userId) {
    throw createError({ statusCode: 400, statusMessage: 'player id is required' })
  }
  const body = ((await readBody(event)) ?? {}) as UserStatusBody
  return handleModerationAdmin(async () => {
    const result = await changeUserModeration({
      actorId: admin.id,
      adminUserIds: configuredAdminUserIds(event),
      userId,
      status: parseUserModerationStatus(body.status),
      reason: parseReason(body.reason),
      expectedRevision: parseExpectedRevision(body.expected_revision),
    })
    if (result.result === 'conflict') {
      throw createError({
        statusCode: 409,
        statusMessage: 'Moderation status was changed by another operation',
        data: { status: result.status, revision: result.revision },
      })
    }
    return result
  })
})
