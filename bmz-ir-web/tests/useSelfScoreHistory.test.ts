import { afterEach, beforeEach, expect, mock, test } from 'bun:test'
import { effectScope, nextTick, ref, shallowRef, type ComputedRef, type EffectScope } from 'vue'

type RequestCall = { url: string; query: Record<string, string | number> }
const requests: { calls: RequestCall[]; immediate: boolean; watch: boolean }[] = []
const scopes: EffectScope[] = []

mock.module('#app', () => ({
  useFetch: (
    url: () => string,
    options: { query: ComputedRef<RequestCall['query']>; immediate: boolean; watch: boolean },
  ) => {
    const request = {
      calls: [] as RequestCall[],
      immediate: options.immediate,
      watch: options.watch,
    }
    requests.push(request)
    return {
      data: shallowRef(),
      pending: ref(false),
      error: shallowRef(null),
      refresh: async () => {
        request.calls.push({ url: url(), query: { ...options.query.value } })
      },
    }
  },
}))
const { useSelfScoreHistory } = await import('../app/composables/useSelfScoreHistory')

beforeEach(() => {
  requests.length = 0
})
afterEach(() => {
  scopes.splice(0).forEach((scope) => scope.stop())
})

function history(url: () => string, filters: () => Record<string, string>) {
  const scope = effectScope()
  scopes.push(scope)
  return scope.run(() => useSelfScoreHistory(url, filters))!
}

test('loads on open and paginates; closed filter changes only reset the page', async () => {
  const policy = ref('AutoLn')
  const state = history(
    () => '/charts/chart/self-scores',
    () => ({ scope: 'self', ln_policy: policy.value }),
  )
  const request = requests[0]!
  expect(request.immediate).toBe(false)
  expect(request.watch).toBe(false)
  expect(request.calls).toHaveLength(0)
  state.page.value = 3
  policy.value = 'ForceCn'
  await nextTick()
  expect(state.page.value).toBe(1)
  expect(request.calls).toHaveLength(0)
  await state.openHistory()
  expect(request.calls[0]?.query).toEqual({
    scope: 'self',
    ln_policy: 'ForceCn',
    limit: 50,
    offset: 0,
  })
  state.page.value = 2
  await nextTick()
  expect(request.calls).toHaveLength(2)
  expect(request.calls[1]?.query.offset).toBe(50)
  state.open.value = false
  policy.value = 'AutoLn'
  await nextTick()
  expect(state.page.value).toBe(1)
  expect(request.calls).toHaveLength(2)
})

test('an open filter or subject change refreshes once at page one', async () => {
  const subject = ref('first')
  const mode = ref('Beatoraja')
  const state = history(
    () => `/charts/${subject.value}/self-scores`,
    () => ({ rule_mode: mode.value }),
  )
  const request = requests[0]!
  await state.openHistory()
  state.page.value = 4
  await nextTick()
  mode.value = 'Dx'
  await nextTick()
  expect(state.page.value).toBe(1)
  expect(request.calls).toHaveLength(3)
  expect(request.calls[2]?.query).toEqual({ rule_mode: 'Dx', limit: 50, offset: 0 })
  mode.value = 'Beatoraja'
  subject.value = 'second'
  await nextTick()
  expect(request.calls).toHaveLength(4)
  expect(request.calls[3]?.url).toBe('/charts/second/self-scores')
})

test('chart and course histories keep separate state and request filters', async () => {
  const gauge = ref('Class')
  const chart = history(
    () => '/charts/chart/self-scores',
    () => ({ scope: 'self' }),
  )
  const course = history(
    () => '/courses/course/self-scores',
    () => ({ gauge: gauge.value }),
  )
  await chart.openHistory()
  expect(course.open.value).toBe(false)
  expect(requests[1]?.calls).toHaveLength(0)
  await course.openHistory()
  gauge.value = 'Hard'
  await nextTick()
  expect(requests[0]?.calls).toHaveLength(1)
  expect(requests[1]?.calls).toHaveLength(2)
  expect(requests[1]?.calls[1]?.query).toEqual({ gauge: 'Hard', limit: 50, offset: 0 })
  scopes[1]?.stop()
  gauge.value = 'Normal'
  await nextTick()
  expect(requests[1]?.calls).toHaveLength(2)
})
