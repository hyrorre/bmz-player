import type { UserModerationStatus } from '#shared/constants/moderation'

interface MeResponse {
  player: {
    id: string
    account_status: UserModerationStatus
    is_admin: boolean
  }
}

/**
 * ログイン中アカウントの状態と管理者権限。管理メニューの表示だけに使い、
 * 権限の判定は管理者APIがサーバー側で行う。
 */
export function useIrAccount() {
  const { user } = useUserSession()
  const requestFetch = useRequestFetch()
  const account = useState<MeResponse['player'] | null>('ir-account', () => null)
  // 取得が完了した（成功・失敗とも）ユーザーID。SSR の描画が取得より先に終わった場合は
  // null のまま渡り、hydration 後のクライアントで取得し直す。
  const loadedFor = useState<string | null>('ir-account-user', () => null)
  const failed = useState('ir-account-failed', () => false)

  async function load() {
    const userId = user.value?.id ?? null
    if (!userId) {
      account.value = null
      loadedFor.value = null
      failed.value = false
      return
    }
    if (loadedFor.value === userId) return
    try {
      const player = (await requestFetch<MeResponse>('/api/v1/me')).player
      if (user.value?.id !== userId) return
      account.value = player
      failed.value = false
    } catch {
      if (user.value?.id !== userId) return
      account.value = null
      failed.value = true
    }
    loadedFor.value = userId
  }

  watch(() => user.value?.id, load, { immediate: true })

  return {
    account,
    failed,
    isAdmin: computed(() => Boolean(account.value?.is_admin)),
    reload: async () => {
      loadedFor.value = null
      await load()
    },
  }
}
