import { getBearerToken } from '../../../utils/auth'
import { revokeToken, revokeWebSession } from '../../../utils/auth_tokens'

interface LogoutBody {
  refresh_token?: string
}

export default defineEventHandler(async (event) => {
  const body = ((await readBody(event).catch(() => undefined)) ?? {}) as LogoutBody
  // 提示された有効な token が属するセッショングループ全体をログアウトする（他の端末には触れない）。
  // 失効済み・期限切れの token では何も変更しないが、クライアントがローカルの token を破棄できる
  // よう、応答は常に成功を返す。
  const accessToken = getBearerToken(event)
  if (accessToken) {
    await revokeToken(accessToken, 'access')
  }
  if (body.refresh_token) {
    await revokeToken(body.refresh_token, 'refresh')
  }

  const session = await getUserSession(event)
  if (session.user?.id && session.secure?.sessionGroupId) {
    await revokeWebSession(session.user.id, session.secure.sessionGroupId)
  }
  await clearUserSession(event)

  return { logged_out: true }
})
