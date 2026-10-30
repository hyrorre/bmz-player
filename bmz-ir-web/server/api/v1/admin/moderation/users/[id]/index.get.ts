import { getUserModerationDetail } from '../../../../../../services/moderation/admin'
import { configuredAdminUserIds, requireIrAdmin } from '../../../../../../utils/admin'

export default defineEventHandler(async (event) => {
  await requireIrAdmin(event)
  const userId = getRouterParam(event, 'id')?.trim()
  if (!userId) {
    throw createError({ statusCode: 400, statusMessage: 'player id is required' })
  }
  const detail = await getUserModerationDetail(userId, configuredAdminUserIds(event))
  if (!detail) {
    throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  }
  return detail
})
