import { computed, readonly, ref, shallowRef } from 'vue'

interface TargetState<T> {
  id: string
  data?: T
  error?: unknown
}

/** 取得した詳細が、要求した対象のものではなかった。 */
export class ModerationTargetMismatchError extends Error {
  constructor(
    readonly requestedId: string,
    readonly receivedId: string,
  ) {
    super('Loaded moderation detail does not match the selected target')
  }
}

/**
 * 管理画面で選択中の1件と、その詳細を結び付けて管理する。
 *
 * 状態変更は必ずこの詳細を元にするため、次を保証する。
 * - 選択を切り替えた時点で古い詳細を捨て、取得中は `detail` を null にする。
 * - 応答が逆順に届いても、最後に要求した選択の結果だけを採用する。
 * - 応答の対象IDが選択と違えば採用せず、エラーとして扱う（古い詳細へ戻さない）。
 */
export function useModerationTarget<T>(
  fetchDetail: (id: string) => Promise<T>,
  idOf: (detail: T) => string,
) {
  const selectedId = ref<string | null>(null)
  const state = shallowRef<TargetState<T> | null>(null)
  let generation = 0

  async function load(id: string) {
    const current = ++generation
    state.value = { id }
    try {
      const data = await fetchDetail(id)
      if (current !== generation || selectedId.value !== id) return
      const receivedId = idOf(data)
      state.value =
        receivedId === id
          ? { id, data }
          : { id, error: new ModerationTargetMismatchError(id, receivedId) }
    } catch (error) {
      if (current !== generation || selectedId.value !== id) return
      state.value = { id, error }
    }
  }

  function select(id: string | null) {
    selectedId.value = id
    if (id) {
      void load(id)
    } else {
      generation++
      state.value = null
    }
  }

  /** 選択中の対象を取得し直す。取得が終わるまで詳細は使えない。 */
  function reload() {
    return selectedId.value ? load(selectedId.value) : Promise.resolve()
  }

  const current = computed(() =>
    state.value && state.value.id === selectedId.value ? state.value : null,
  )
  const detail = computed(() => current.value?.data ?? null)
  const error = computed(() => current.value?.error ?? null)
  const loading = computed(
    () => selectedId.value !== null && !current.value?.data && !current.value?.error,
  )

  return {
    selectedId: readonly(selectedId),
    detail,
    error,
    loading,
    select,
    reload,
  }
}
