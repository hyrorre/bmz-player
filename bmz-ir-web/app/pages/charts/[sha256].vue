<script setup lang="ts">
import type { IrRanking, LnScorePolicy } from '~~/bmz-ir-web/shared/types/ir'

interface ChartDetail {
  chart: {
    sha256: string
    title: string
    subtitle: string | null
    genre: string | null
    artist: string | null
    mode: string
    level: number | null
    notes: number
  }
  stats: {
    global: { play_count: number; clear_count: number }
    self: { play_count: number; clear_count: number } | null
  }
}

const route = useRoute()
const sha256 = computed(() => String(route.params.sha256 ?? ''))

const gauge = ref('normal')
const lnPolicy = ref<LnScorePolicy>('ForceLn')
const gauges = ['normal', 'easy', 'hard', 'ex_hard', 'assist_easy', 'hazard']
const lnPolicies: LnScorePolicy[] = ['AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn']

const { data: detail, error: detailError } = await useFetch<ChartDetail>(
  () => `/api/v1/charts/${sha256.value}`,
)
const {
  data: ranking,
  pending: rankingPending,
  error: rankingError,
} = await useFetch<IrRanking>(() => `/api/v1/charts/${sha256.value}/ranking`, {
  query: computed(() => ({ scope: 'global', gauge: gauge.value, ln_policy: lnPolicy.value })),
})
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <UAlert v-if="detailError" color="error" :description="detailError.message" class="mb-6" />
      <template v-else-if="detail">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink to="/charts" class="hover:underline">譜面一覧</NuxtLink>
          </p>
          <h1 class="text-3xl font-semibold">
            {{ detail.chart.title }}
            <span v-if="detail.chart.subtitle" class="text-xl text-neutral-400">
              {{ detail.chart.subtitle }}
            </span>
          </h1>
          <p class="mt-2 text-sm text-neutral-300">
            {{ detail.chart.artist ?? '' }}
            <span v-if="detail.chart.genre"> / {{ detail.chart.genre }}</span>
          </p>
          <p class="mt-1 text-sm text-neutral-400">
            {{ detail.chart.mode }}
            <span v-if="detail.chart.level != null"> ☆{{ detail.chart.level }}</span>
            ・ {{ detail.chart.notes }} notes
            ・ プレイ {{ detail.stats.global.play_count }} / クリア
            {{ detail.stats.global.clear_count }}
          </p>
        </div>

        <div class="mb-4 flex flex-wrap gap-3">
          <USelect v-model="gauge" :items="gauges" class="w-40" />
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
                <th class="px-3 py-2">入力</th>
                <th class="px-3 py-2">日時</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="entry in ranking.ranking.entries"
                :key="entry.score.score_id"
                class="border-t border-neutral-800"
                :class="{ 'bg-primary-950/40': entry.relation.is_self }"
              >
                <td class="px-3 py-2 text-neutral-300">{{ entry.rank }}</td>
                <td class="px-3 py-2">
                  <NuxtLink :to="`/players/${entry.player.id}`" class="hover:underline">
                    {{ entry.player.display_name }}
                  </NuxtLink>
                  <UBadge v-if="entry.relation.is_rival" size="sm" color="warning" variant="subtle">
                    rival
                  </UBadge>
                </td>
                <td class="px-3 py-2 text-right font-medium">{{ entry.score.ex_score }}</td>
                <td class="px-3 py-2">{{ entry.score.clear }}</td>
                <td class="px-3 py-2 text-right">{{ entry.score.max_combo }}</td>
                <td class="px-3 py-2 text-right">{{ entry.score.min_bp }}</td>
                <td class="px-3 py-2 text-neutral-400">{{ entry.score.device_type }}</td>
                <td class="px-3 py-2 text-neutral-400">
                  {{ entry.score.played_at ? new Date(entry.score.played_at).toLocaleString() : '-' }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </section>
  </main>
</template>
