import { getQuery } from 'h3'
import {
  parsePage,
  parseSearch,
  parseUserModerationStatus,
  searchUsers,
} from '../../../../../services/moderation/admin'
import { handleModerationAdmin } from '../../../../../services/moderation/http'
import { requireIrAdmin } from '../../../../../utils/admin'

/** ユーザー検索 (ID完全一致 / 表示名・メールアドレス部分一致)。メールアドレスは管理者APIだけが返す。 */
export default defineEventHandler(async (event) => {
  await requireIrAdmin(event)
  const query = getQuery(event)
  return handleModerationAdmin(() =>
    searchUsers({
      search: parseSearch(query.q),
      status: query.status ? parseUserModerationStatus(query.status) : undefined,
      ...parsePage(query),
    }),
  )
})
