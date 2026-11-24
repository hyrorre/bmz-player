import type { LocationQuery } from 'vue-router'
import {
  listSortOptions,
  readListSort,
  type ListSortOrder,
  type SortableList,
} from '../../shared/utils/list_sort'

const listPageSize = 50

export function useRoutePagination({
  maxOffset = Number.MAX_SAFE_INTEGER,
  pageSize = listPageSize,
  queryKey = 'page',
} = {}) {
  const route = useRoute()
  const maxPage = Math.floor(maxOffset / pageSize) + 1
  const page = computed(() => Math.min(readPage(route.query[queryKey], pageSize), maxPage))
  const offset = computed(() => (page.value - 1) * pageSize)

  function pageLocation(nextPage: number) {
    return { path: route.path, query: { ...route.query, [queryKey]: pageParam(nextPage) } }
  }

  return { page, pageSize, maxPage, offset, pageLocation }
}

export function useSearchableListQuery(list: SortableList) {
  const route = useRoute()
  const { t } = useI18n()
  const pagination = useRoutePagination()
  const appliedSearch = computed(() => searchParam(route.query.q))
  const search = ref(appliedSearch.value ?? '')
  watch([appliedSearch, pagination.page], () => {
    search.value = appliedSearch.value ?? ''
  })

  const listQuery = computed(() => listReturnQuery(route.query))
  const sorting = computed(() => readListSort(list, route.query))
  const sortItems = computed(() =>
    listSortOptions(list, route.query).map(([value]) => ({
      value,
      label: t(value === 'title' ? `listSort.${list}_title` : `listSort.${value}`),
    })),
  )
  const sort = computed({
    get: () => sorting.value.sort,
    set(value: string) {
      if (value === sorting.value.sort) return
      const next = readListSort(list, { ...route.query, sort: value, order: undefined })
      void navigateTo({ path: route.path, query: { ...route.query, ...next, page: undefined } })
    },
  })
  const order = computed({
    get: () => sorting.value.order,
    set(value: ListSortOrder) {
      if (value === sorting.value.order) return
      void navigateTo({
        path: route.path,
        query: { ...route.query, sort: sorting.value.sort, order: value, page: undefined },
      })
    },
  })

  async function applySearch(refresh: () => Promise<void>) {
    const q = searchParam(search.value)
    if (pagination.page.value === 1 && q === appliedSearch.value) {
      await refresh()
    } else {
      await navigateTo({ path: route.path, query: { ...route.query, q, page: undefined } })
    }
  }

  return {
    ...pagination,
    search,
    listQuery,
    sort,
    order,
    sortItems,
    apiQuery: computed(() => ({
      q: appliedSearch.value,
      ...sorting.value,
      offset: pagination.offset.value,
      limit: pagination.pageSize,
    })),
    applySearch,
  }
}

export function listReturnQuery(query: LocationQuery) {
  return {
    q: searchParam(query.q),
    page: pageParam(readPage(query.page)),
    sort: searchParam(query.sort),
    order: query.order === 'asc' || query.order === 'desc' ? query.order : undefined,
  }
}

function searchParam(value: unknown) {
  return typeof value === 'string' ? value.trim() || undefined : undefined
}

function readPage(value: unknown, pageSize = listPageSize) {
  if (typeof value !== 'string' || !/^[1-9]\d*$/.test(value)) return 1
  const parsed = Number(value)
  const maxSafePage = Math.floor(Number.MAX_SAFE_INTEGER / pageSize) + 1
  return Number.isSafeInteger(parsed) && parsed <= maxSafePage ? parsed : 1
}

function pageParam(page: number) {
  return page > 1 ? String(page) : undefined
}
