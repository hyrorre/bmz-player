<script setup lang="ts">
interface CourseListItem {
  course_hash: string
  title: string
  kind: string
  chart_count: number
  updated_at: string
}

const { data, pending, error } = await useFetch<{ courses: CourseListItem[] }>('/api/v1/courses')
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <div class="mb-8">
        <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
        <h1 class="text-3xl font-semibold">コース / 段位一覧</h1>
        <p class="mt-2 text-sm text-neutral-300">
          スコアが登録されているコースのランキングを確認できます。
        </p>
      </div>

      <UAlert v-if="error" color="error" :description="error.message" class="mb-6" />
      <p v-else-if="pending" class="text-sm text-neutral-400">読み込み中...</p>
      <p v-else-if="!data?.courses.length" class="text-sm text-neutral-400">
        まだコーススコアが登録されていません。
      </p>

      <ul v-else class="divide-y divide-neutral-800 rounded-lg border border-neutral-800">
        <li v-for="course in data.courses" :key="course.course_hash">
          <NuxtLink
            :to="`/courses/${course.course_hash}`"
            class="flex items-center justify-between gap-4 px-4 py-3 hover:bg-neutral-900"
          >
            <div class="min-w-0">
              <p class="truncate font-medium">{{ course.title || '(無題)' }}</p>
              <p class="text-sm text-neutral-400">{{ course.chart_count }} 曲</p>
            </div>
            <UBadge
              :color="course.kind === 'dan' ? 'warning' : 'neutral'"
              size="sm"
              variant="subtle"
            >
              {{ course.kind === 'dan' ? '段位' : 'コース' }}
            </UBadge>
          </NuxtLink>
        </li>
      </ul>
    </section>
  </main>
</template>
