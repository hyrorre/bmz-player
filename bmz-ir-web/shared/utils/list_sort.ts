import { readDifficultyFilter } from './difficulty_filter'

export type ListSortOrder = 'asc' | 'desc'

const sortOptions = {
  players: {
    last_score_at: 'desc',
    display_name: 'asc',
    created_at: 'desc',
    best_score_count: 'desc',
    best_course_score_count: 'desc',
  },
  charts: {
    created_at: 'desc',
    title: 'asc',
    artist: 'asc',
    level: 'asc',
    notes: 'asc',
    table_level: 'asc',
  },
  courses: {
    created_at: 'desc',
    title: 'asc',
    chart_count: 'asc',
  },
} as const

export type SortableList = keyof typeof sortOptions
type ListSortKey<List extends SortableList> = List extends SortableList
  ? keyof (typeof sortOptions)[List] & string
  : never

export function listSortOptions<List extends SortableList>(
  list: List,
  query: Record<string, unknown>,
) {
  const options = Object.entries(sortOptions[list]) as [ListSortKey<List>, ListSortOrder][]
  return options.filter(
    ([key]) => key !== 'table_level' || readDifficultyFilter(query) !== undefined,
  )
}

export function readListSort<List extends SortableList>(
  list: List,
  query: Record<string, unknown>,
) {
  const options = listSortOptions(list, query)
  const requested = options.find(([key]) => key === query.sort)
  const [sort, defaultOrder] = requested ?? options[0]!
  const order =
    (requested || query.sort === undefined) && (query.order === 'asc' || query.order === 'desc')
      ? query.order
      : defaultOrder
  return { sort, order }
}
