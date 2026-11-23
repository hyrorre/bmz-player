<script setup lang="ts">
interface CourseListItem {
  course_hash: string
  title: string
  kind: string
  chart_count: number
  updated_at: string
}

interface CourseListResult {
  courses: CourseListItem[]
  pagination: {
    limit: number
    offset: number
    total: number
    has_more: boolean
  }
}

const { search, page, pageSize, pageLocation, listQuery, apiQuery, applySearch } =
  useSearchableListQuery()
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()
const { data, pending, error, refresh } = await useFetch<CourseListResult>('/api/v1/courses', {
  query: apiQuery,
})

const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.coursesLoadFailed') : '',
)
useSeoMeta({ title: () => t('courses.title') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <div class="mb-8">
        <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
        <h1 class="text-3xl font-semibold">{{ t('courses.title') }}</h1>
        <p class="mt-2 text-sm text-neutral-300">
          {{ t('courses.description') }}
        </p>
      </div>

      <form class="mb-6 flex gap-3" @submit.prevent="applySearch(refresh)">
        <UInput
          v-model="search"
          class="flex-1"
          icon="i-lucide-search"
          :placeholder="t('courses.searchPlaceholder')"
          :aria-label="t('courses.searchPlaceholder')"
        />
        <UButton type="submit" color="primary" variant="subtle">{{ t('common.search') }}</UButton>
      </form>

      <UAlert v-if="error" color="error" :description="errorDescription" class="mb-6" />
      <p v-else-if="pending" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
      <div v-else-if="!data?.courses.length" class="text-sm text-muted">
        <p>{{ t('courses.empty') }}</p>
        <UPagination
          v-if="page > 1"
          :page="1"
          :to="pageLocation"
          :total="1"
          :show-controls="false"
          class="mt-3"
        />
      </div>

      <ul v-else class="divide-y divide-neutral-800 rounded-lg border border-neutral-800">
        <li v-for="course in data.courses" :key="course.course_hash">
          <NuxtLink
            :to="localePath({ path: `/courses/${course.course_hash}`, query: listQuery })"
            class="flex items-center justify-between gap-4 px-4 py-3 hover:bg-neutral-900"
          >
            <div class="min-w-0">
              <p class="truncate font-medium">{{ course.title || t('common.untitled') }}</p>
              <p class="text-sm text-neutral-400">
                {{ t('courses.chartCount', { count: course.chart_count }) }}
              </p>
            </div>
            <UBadge
              :color="course.kind === 'dan' ? 'warning' : 'neutral'"
              size="sm"
              variant="subtle"
            >
              {{ course.kind === 'dan' ? t('courses.dan') : t('courses.course') }}
            </UBadge>
          </NuxtLink>
        </li>
      </ul>

      <div
        v-if="data?.courses.length && data.pagination.total > pageSize"
        class="mt-6 flex justify-end"
      >
        <UPagination
          :page="page"
          :to="pageLocation"
          :sibling-count="1"
          :items-per-page="pageSize"
          :total="data.pagination.total"
        />
      </div>
    </section>
  </main>
</template>
