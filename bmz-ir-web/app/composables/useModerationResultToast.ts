/** 状態変更の結果 (changed / unchanged / conflict) を件数ごとに通知する。 */
export function useModerationResultToast() {
  const toast = useToast()
  const { t } = useI18n()
  const { translateApiError } = useApiError()

  function notifyResults(results: Array<{ result: 'changed' | 'unchanged' | 'conflict' }>) {
    const count = (result: string) => results.filter((item) => item.result === result).length
    const changed = count('changed')
    const unchanged = count('unchanged')
    const conflict = count('conflict')
    if (changed > 0) {
      toast.add({ title: t('admin.result.changed', { count: changed }), color: 'success' })
    }
    if (unchanged > 0) {
      toast.add({ title: t('admin.result.unchanged', { count: unchanged }), color: 'info' })
    }
    if (conflict > 0) {
      toast.add({ title: t('admin.result.conflict', { count: conflict }), color: 'warning' })
    }
  }

  function notifyError(error: unknown) {
    toast.add({
      title: t('admin.result.failed'),
      description: translateApiError(error, 'admin.result.failed'),
      color: 'error',
    })
  }

  return { notifyResults, notifyError }
}
