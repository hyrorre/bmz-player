<script setup lang="ts">
interface CourseDetail {
  course: {
    course_hash: string
    title: string
    kind: string
    charts: string[]
    chart_count: number
    constraints: Record<string, unknown>
  }
  stats: { play_count: number }
}

interface CourseRankingEntry {
  rank: number
  player: { id: string; display_name: string }
  score: {
    course_score_id: string
    clear: string
    course_clear: boolean
    ex_score: number
    max_combo: number
    bp: number
    device_type: string
    played_at: string | null
    verification: string
  }
}

interface CourseRanking {
  ranking: { entries: CourseRankingEntry[] }
}

const route = useRoute()
const courseHash = computed(() => String(route.params.hash ?? ''))

const gauge = ref('Class')
const lnPolicy = ref('AutoLn')
const gauges = ['Class', 'ExClass', 'ExHardClass', 'Normal', 'Hard']
const lnPolicies = ['AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn']

const { data: detail, error: detailError } = await useFetch<CourseDetail>(
  () => `/api/v1/courses/${courseHash.value}`,
)
const {
  data: ranking,
  pending: rankingPending,
  error: rankingError,
} = await useFetch<CourseRanking>(() => `/api/v1/courses/${courseHash.value}/ranking`, {
  query: computed(() => ({ gauge: gauge.value, ln_policy: lnPolicy.value })),
})
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <UAlert v-if="detailError" color="error" :description="detailError.message" class="mb-6" />
      <template v-else-if="detail">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink to="/courses" class="hover:underline">コース一覧</NuxtLink>
          </p>
          <h1 class="text-3xl font-semibold">
            {{ detail.course.title || '(無題)' }}
            <UBadge
              :color="detail.course.kind === 'dan' ? 'warning' : 'neutral'"
              size="md"
              variant="subtle"
            >
              {{ detail.course.kind === 'dan' ? '段位' : 'コース' }}
            </UBadge>
          </h1>
          <p class="mt-2 text-sm text-neutral-400">
            {{ detail.course.chart_count }} 曲 ・ プレイ {{ detail.stats.play_count }} 回
          </p>
        </div>

        <h2 class="mb-2 text-lg font-medium">構成譜面</h2>
        <ol class="mb-8 list-inside list-decimal space-y-1 text-sm">
          <li v-for="sha in detail.course.charts" :key="sha">
            <NuxtLink :to="`/charts/${sha}`" class="font-mono hover:underline">
              {{ sha.slice(0, 16) }}…
            </NuxtLink>
          </li>
        </ol>

        <div class="mb-4 flex flex-wrap gap-3">
          <USelect v-model="gauge" :items="gauges" class="w-44" />
          <USelect v-model="lnPolicy" :items="lnPolicies" class="w-40" />
        </div>

        <UAlert v-if="rankingError" color="error" :description="rankingError.message" />
        <p v-else-if="rankingPending" class="text-sm text-neutral-400">ランキング読み込み中...</p>
        <p v-else-if="!ranking?.ranking.entries.length" class="text-sm text-neutral-400">
          この条件のスコアはまだありません。
        </p>
        <div v-else class="overflow-x-auto rounded-lg border border-neutral-800">
          <table class="w-full text-sm">
            <thead class="bg-neutral-900 text-left text-neutral-300">
              <tr>
                <th class="px-3 py-2">#</th>
                <th class="px-3 py-2">プレイヤー</th>
                <th class="px-3 py-2 text-right">EX</th>
                <th class="px-3 py-2">クリア</th>
                <th class="px-3 py-2 text-right">COMBO</th>
                <th class="px-3 py-2 text-right">BP</th>
                <th class="px-3 py-2">日時</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="entry in ranking.ranking.entries"
                :key="entry.score.course_score_id"
                class="border-t border-neutral-800"
              >
                <td class="px-3 py-2 text-neutral-300">{{ entry.rank }}</td>
                <td class="px-3 py-2">
                  <NuxtLink :to="`/players/${entry.player.id}`" class="hover:underline">
                    {{ entry.player.display_name }}
                  </NuxtLink>
                </td>
                <td class="px-3 py-2 text-right font-medium">{{ entry.score.ex_score }}</td>
                <td class="px-3 py-2">
                  {{ entry.score.clear }}
                  <UBadge
                    v-if="entry.score.course_clear"
                    color="success"
                    size="sm"
                    variant="subtle"
                  >
                    CLEAR
                  </UBadge>
                </td>
                <td class="px-3 py-2 text-right">{{ entry.score.max_combo }}</td>
                <td class="px-3 py-2 text-right">{{ entry.score.bp }}</td>
                <td class="px-3 py-2 text-neutral-400">
                  {{
                    entry.score.played_at ? new Date(entry.score.played_at).toLocaleString() : '-'
                  }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </section>
  </main>
</template>
