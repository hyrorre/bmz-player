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
  // SSR でも cookie を転送して取得し、hydration 後に再取得しない。
  const requestFetch = useRequestFetch()
  const account = useState<MeResponse['player'] | null>('ir-account', () => null)
  const loadedFor = useState<string | null>('ir-account-user', () => null)

  async function load() {
    const userId = user.value?.id ?? null
    if (!userId) {
      account.value = null
      loadedFor.value = null
      return
    }
    if (loadedFor.value === userId) return
    loadedFor.value = userId
    try {
      account.value = (await requestFetch<MeResponse>('/api/v1/me')).player
    } catch {
      account.value = null
    }
  }

  watch(() => user.value?.id, load, { immediate: true })

  return {
    account,
    isAdmin: computed(() => Boolean(account.value?.is_admin)),
    reload: async () => {
      loadedFor.value = null
      await load()
    },
  }
}
