import { getQuery } from 'h3'
import {
  ModerationAdminError,
  listAuditLogs,
  parsePage,
  parseSearch,
} from '../../../../services/moderation/admin'
import { handleModerationAdmin } from '../../../../services/moderation/http'
import { requireIrAdmin } from '../../../../utils/admin'

/** 監査ログ一覧 (新しい順)。対象種別・対象ID・操作した管理者で絞り込める。 */
export default defineEventHandler(async (event) => {
  await requireIrAdmin(event)
  const query = getQuery(event)
  return handleModerationAdmin(() => {
    const targetType = parseSearch(query.target_type)
    if (targetType && targetType !== 'user' && targetType !== 'chart') {
      throw new ModerationAdminError(400, 'target_type is invalid')
    }
    const targetId = parseSearch(query.target_id)
    return listAuditLogs({
      targetType: (targetType || undefined) as 'user' | 'chart' | undefined,
      targetId:
        targetType === 'chart' ? targetId.toLowerCase() || undefined : targetId || undefined,
      actorId: parseSearch(query.actor_id) || undefined,
      ...parsePage(query),
    })
  })
})
