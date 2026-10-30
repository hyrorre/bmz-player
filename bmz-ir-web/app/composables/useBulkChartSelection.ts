import { computed, shallowRef } from 'vue'

export interface BulkChartSelectionEntry {
  label: string
  /** 選択した時点で管理者が見ていた revision。一覧を再取得しても書き換えない。 */
  revision: number
}

export interface BulkChartSelectable {
  sha256: string
  revision: number
}

/**
 * 譜面の一括操作の対象。ページ移動・検索・一覧の再取得をまたいで、選択した時点の
 * revision を保持する。他の管理者が変更した譜面は送信時に conflict になり、選択から
 * 外して競合一覧へ移す。再操作するには、最新の状態を見た上で選択し直す。
 */
export function useBulkChartSelection() {
  const selected = shallowRef(new Map<string, BulkChartSelectionEntry>())
  const conflicts = shallowRef(new Map<string, { label: string }>())

  function select(chart: BulkChartSelectable, label: string) {
    const next = new Map(selected.value)
    next.set(chart.sha256, { label, revision: chart.revision })
    selected.value = next
    if (conflicts.value.has(chart.sha256)) {
      const nextConflicts = new Map(conflicts.value)
      nextConflicts.delete(chart.sha256)
      conflicts.value = nextConflicts
    }
  }

  function deselect(sha256: string) {
    if (!selected.value.has(sha256)) return
    const next = new Map(selected.value)
    next.delete(sha256)
    selected.value = next
  }

  function clear() {
    selected.value = new Map()
  }

  function dismissConflicts() {
    conflicts.value = new Map()
  }

  /** 確認ダイアログへ渡す、送信内容の固定コピー。 */
  function snapshot() {
    return [...selected.value.entries()].map(([sha256, entry]) => ({ sha256, ...entry }))
  }

  /**
   * 一括操作の結果を反映する。適用済み・同じ状態の譜面は選択から外し、競合した譜面は
   * 古い revision のまま再送できないよう、選択から外して競合一覧へ移す。
   */
  function applyResults(
    results: Array<{ sha256: string; result: 'changed' | 'unchanged' | 'conflict' }>,
    labels: Map<string, string>,
  ) {
    const next = new Map(selected.value)
    const nextConflicts = new Map(conflicts.value)
    for (const result of results) {
      next.delete(result.sha256)
      if (result.result === 'conflict') {
        nextConflicts.set(result.sha256, { label: labels.get(result.sha256) ?? result.sha256 })
      } else {
        nextConflicts.delete(result.sha256)
      }
    }
    selected.value = next
    conflicts.value = nextConflicts
  }

  return {
    selected,
    conflicts,
    size: computed(() => selected.value.size),
    has: (sha256: string) => selected.value.has(sha256),
    select,
    deselect,
    clear,
    dismissConflicts,
    snapshot,
    applyResults,
  }
}
