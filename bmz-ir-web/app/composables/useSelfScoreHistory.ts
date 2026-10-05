import { useFetch } from '#app'
import { computed, ref, watch } from 'vue'

/** Load only while the history dialog is open; reset pagination when its subject changes. */
export function useSelfScoreHistory<T>(url: () => string, filters: () => Record<string, string>) {
  const open = ref(false)
  const page = ref(1)
  const limit = 50
  const query = computed(() => ({
    ...filters(),
    limit,
    offset: (page.value - 1) * limit,
  }))
  // Keep useFetch synchronous inside the wrapper so all watchers belong to the caller's scope.
  const { data, pending, error, refresh } = useFetch<T>(url, {
    immediate: false,
    watch: false,
    query,
  })

  watch([url, filters], () => {
    if (!open.value) {
      page.value = 1
      return
    }
    if (page.value === 1) {
      void refresh()
    } else {
      page.value = 1
    }
  })
  watch(page, () => {
    if (open.value) void refresh()
  })

  async function openHistory() {
    open.value = true
    await refresh()
  }

  return { open, page, limit, data, pending, error, openHistory }
}
