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

const search = ref('')
const appliedSearch = ref('')
const page = ref(1)
const pageSize = 50
const offset = computed(() => (page.value - 1) * pageSize)
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()
const { data, pending, error, refresh } = await useFetch<CourseListResult>('/api/v1/courses', {
  query: computed(() => ({
    limit: pageSize,
    offset: offset.value,
    ...(appliedSearch.value ? { q: appliedSearch.value } : {}),
  })),
  watch: false,
})

const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.coursesLoadFailed') : '',
)
useSeoMeta({ title: () => t('courses.title') })

function applySearch() {
  appliedSearch.value = search.value.trim()
  if (page.value === 1) {
    refresh()
  } else {
    page.value = 1
  }
}

watch(page, () => {
  refresh()
})
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

      <div class="mb-6 flex gap-3">
        <UInput
          v-model="search"
          class="flex-1"
          icon="i-lucide-search"
          :placeholder="t('courses.searchPlaceholder')"
          @keydown.enter="applySearch"
        />
        <UButton color="primary" variant="subtle" @click="applySearch">{{
          t('common.search')
        }}</UButton>
      </div>

      <UAlert v-if="error" color="error" :description="errorDescription" class="mb-6" />
      <p v-else-if="pending" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
      <p v-else-if="!data?.courses.length" class="text-sm text-neutral-400">
        {{ t('courses.empty') }}
      </p>

      <ul v-else class="divide-y divide-neutral-800 rounded-lg border border-neutral-800">
        <li v-for="course in data.courses" :key="course.course_hash">
          <NuxtLink
            :to="localePath(`/courses/${course.course_hash}`)"
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

      <div v-if="data && data.pagination.total > pageSize" class="mt-6 flex justify-end">
        <UPagination
          v-model:page="page"
          :items-per-page="pageSize"
          :total="data.pagination.total"
        />
      </div>
    </section>
  </main>
</template>
