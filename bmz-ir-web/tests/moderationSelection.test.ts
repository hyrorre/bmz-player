import { describe, expect, test } from 'bun:test'
import { useBulkChartSelection } from '../app/composables/useBulkChartSelection'
import {
  ModerationTargetMismatchError,
  useModerationTarget,
} from '../app/composables/useModerationTarget'

interface Detail {
  id: string
  revision: number
}

/** 応答の順序をテストから制御できる詳細取得。 */
function controlledFetch() {
  const pending = new Map<
    string,
    Array<{ resolve: (d: Detail) => void; reject: (e: unknown) => void }>
  >()
  const fetchDetail = (id: string) =>
    new Promise<Detail>((resolve, reject) => {
      pending.set(id, [...(pending.get(id) ?? []), { resolve, reject }])
    })
  const take = (id: string) => {
    const queue = pending.get(id) ?? []
    const next = queue.shift()
    if (!next) throw new Error(`no pending request for ${id}`)
    return next
  }
  return {
    fetchDetail,
    resolve: async (id: string, detail: Detail = { id, revision: 1 }) => {
      take(id).resolve(detail)
      await Promise.resolve()
      await Promise.resolve()
    },
    reject: async (id: string, error: unknown) => {
      take(id).reject(error)
      await Promise.resolve()
      await Promise.resolve()
    },
  }
}

function target(server = controlledFetch()) {
  return { server, view: useModerationTarget(server.fetchDetail, (detail) => detail.id) }
}

describe('moderation target', () => {
  test('switching targets drops the previous detail until the new one loads', async () => {
    const { server, view } = target()
    view.select('alice')
    expect(view.loading.value).toBe(true)
    await server.resolve('alice')
    expect(view.detail.value?.id).toBe('alice')

    view.select('bob')
    // 取得中は古い alice の詳細を操作に使えない。
    expect(view.detail.value).toBeNull()
    expect(view.loading.value).toBe(true)
    await server.resolve('bob')
    expect(view.detail.value?.id).toBe('bob')
  })

  test('a slower response for an earlier selection is ignored', async () => {
    const { server, view } = target()
    view.select('alice')
    view.select('bob')
    await server.resolve('bob', { id: 'bob', revision: 2 })
    await server.resolve('alice', { id: 'alice', revision: 9 })
    expect(view.selectedId.value).toBe('bob')
    expect(view.detail.value).toEqual({ id: 'bob', revision: 2 })
  })

  test('A to B to A uses only the response for the last request', async () => {
    const { server, view } = target()
    view.select('alice')
    view.select('bob')
    view.select('alice')
    await server.resolve('alice', { id: 'alice', revision: 1 })
    // 1回目の alice の応答は、最新の要求ではないので採用しない。
    expect(view.detail.value).toBeNull()
    await server.resolve('bob')
    expect(view.detail.value).toBeNull()
    await server.resolve('alice', { id: 'alice', revision: 3 })
    expect(view.detail.value).toEqual({ id: 'alice', revision: 3 })
  })

  test('a failed load shows the error instead of the previous detail', async () => {
    const { server, view } = target()
    view.select('alice')
    await server.resolve('alice')
    view.select('bob')
    await server.reject('bob', new Error('boom'))
    expect(view.detail.value).toBeNull()
    expect(view.loading.value).toBe(false)
    expect(view.error.value).toBeInstanceOf(Error)
  })

  test('a detail for a different target is rejected', async () => {
    const { server, view } = target()
    view.select('alice')
    await server.resolve('alice', { id: 'mallory', revision: 1 })
    expect(view.detail.value).toBeNull()
    expect(view.error.value).toBeInstanceOf(ModerationTargetMismatchError)
  })

  test('reloading hides the detail until the fresh revision arrives', async () => {
    const { server, view } = target()
    view.select('alice')
    await server.resolve('alice', { id: 'alice', revision: 1 })
    const reloaded = view.reload()
    expect(view.detail.value).toBeNull()
    await server.resolve('alice', { id: 'alice', revision: 2 })
    await reloaded
    expect(view.detail.value?.revision).toBe(2)

    view.select(null)
    expect(view.detail.value).toBeNull()
    expect(view.loading.value).toBe(false)
  })
})

describe('bulk chart selection', () => {
  const chart = (sha256: string, revision: number) => ({ sha256, revision })

  test('keeps the revision seen when the chart was selected', () => {
    const bulk = useBulkChartSelection()
    bulk.select(chart('a', 1), 'A')
    bulk.select(chart('b', 0), 'B')
    // 一覧の再取得・ページ移動・検索の変更では選択を書き換えない（自動更新しない）。
    expect(bulk.snapshot()).toEqual([
      { sha256: 'a', label: 'A', revision: 1 },
      { sha256: 'b', label: 'B', revision: 0 },
    ])
    bulk.deselect('b')
    expect(bulk.size.value).toBe(1)
  })

  test('changed and unchanged charts leave the selection; conflicts need a new selection', () => {
    const bulk = useBulkChartSelection()
    bulk.select(chart('a', 1), 'A')
    bulk.select(chart('b', 2), 'B')
    bulk.select(chart('c', 3), 'C')
    bulk.applyResults(
      [
        { sha256: 'a', result: 'changed' },
        { sha256: 'b', result: 'unchanged' },
        { sha256: 'c', result: 'conflict' },
      ],
      new Map([
        ['a', 'A'],
        ['b', 'B'],
        ['c', 'C'],
      ]),
    )
    expect(bulk.size.value).toBe(0)
    expect([...bulk.conflicts.value.keys()]).toEqual(['c'])
    // 古い revision のまま再送はできず、最新の一覧から選び直すと競合一覧から外れる。
    expect(bulk.snapshot()).toEqual([])
    bulk.select(chart('c', 4), 'C')
    expect(bulk.snapshot()).toEqual([{ sha256: 'c', label: 'C', revision: 4 }])
    expect(bulk.conflicts.value.size).toBe(0)
  })

  test('a snapshot taken for the confirmation is not affected by later selection changes', () => {
    const bulk = useBulkChartSelection()
    bulk.select(chart('a', 1), 'A')
    const snapshot = bulk.snapshot()
    bulk.select(chart('a', 5), 'A')
    bulk.select(chart('b', 1), 'B')
    expect(snapshot).toEqual([{ sha256: 'a', label: 'A', revision: 1 }])
  })
})
