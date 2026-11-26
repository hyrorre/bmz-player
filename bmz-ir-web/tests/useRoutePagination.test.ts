import { afterEach, beforeEach, expect, mock, test } from 'bun:test'
import { createRenderer, nextTick, reactive, ref, type App } from 'vue'
import type { LocationQuery, RouteLocationRaw } from 'vue-router'
import { useRoutePagination } from '../app/composables/useRoutePagination'

const route = reactive({ path: '/players/player', query: {} as LocationQuery })
const navigateTo = mock((_location: RouteLocationRaw, _options: { replace: boolean }) => {})

const noop = () => {}
const renderer = createRenderer({
  createComment: () => ({}),
  createElement: () => ({}),
  createText: () => ({}),
  insert: noop,
  nextSibling: () => null,
  parentNode: () => null,
  patchProp: noop,
  remove: noop,
  setElementText: noop,
  setText: noop,
})
let app: App | undefined

beforeEach(() => {
  // Nuxt injects these imports in the application build.
  Object.assign(globalThis, { useRoute: () => route, navigateTo })
})

afterEach(() => {
  app?.unmount()
  navigateTo.mockClear()
  Reflect.deleteProperty(globalThis, 'useRoute')
  Reflect.deleteProperty(globalThis, 'navigateTo')
})

test('keeps the filters and replaces an out-of-range page after its own result arrives', async () => {
  route.query = { score_page: '3', rule_mode: 'Dx' }
  const result = ref({ offset: 0, total: 1 })
  app = renderer.createApp({
    setup() {
      const pagination = useRoutePagination({ queryKey: 'score_page', pageSize: 50 })
      void pagination.correctOutOfRangePage(() => result.value)
      return () => null
    },
  })
  app.mount({})
  expect(navigateTo).not.toHaveBeenCalled()

  result.value = { offset: 100, total: 60 }
  await nextTick()
  expect(navigateTo).toHaveBeenCalledWith(
    { path: route.path, query: { score_page: '2', rule_mode: 'Dx' } },
    { replace: true },
  )

  route.query.score_page = '2'
  result.value = { offset: 50, total: 60 }
  await nextTick()
  expect(navigateTo).toHaveBeenCalledTimes(1)

  result.value = { offset: 50, total: 0 }
  await nextTick()
  expect(navigateTo).toHaveBeenLastCalledWith(
    { path: route.path, query: { score_page: undefined, rule_mode: 'Dx' } },
    { replace: true },
  )
})
