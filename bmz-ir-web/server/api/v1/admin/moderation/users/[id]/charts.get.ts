import { getQuery } from 'h3'
import { listUserCharts, parsePage } from '../../../../../../services/moderation/admin'
import { handleModerationAdmin } from '../../../../../../services/moderation/http'
import { requireIrAdmin } from '../../../../../../utils/admin'

/** ユーザーがスコアを送った譜面の一覧。`sole=1` でそのユーザーだけが送った譜面に絞る。 */
export default defineEventHandler(async (event) => {
  await requireIrAdmin(event)
  const userId = getRouterParam(event, 'id')?.trim()
  if (!userId) {
    throw createError({ statusCode: 400, statusMessage: 'player id is required' })
  }
  const query = getQuery(event)
  return handleModerationAdmin(() =>
    listUserCharts({
      userId,
      soleOnly: query.sole === '1' || query.sole === 'true',
      ...parsePage(query),
    }),
  )
})
