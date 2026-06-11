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
const chartParam = computed(() => String(route.params.sha256 ?? '').trim().toLowerCase())
const paramError = ref<string | null>(null)

if (/^[0-9a-f]{32}$/.test(chartParam.value)) {
  try {
    const lookup = await $fetch<{ sha256: string }>('/api/v1/charts/lookup', {
      query: { md5: chartParam.value },
    })
    await navigateTo(`/charts/${lookup.sha256}`, { redirectCode: 301, replace: true })
  } catch {
    paramError.value = '指定された MD5 の譜面は見つかりません'
  }
} else if (chartParam.value && !/^[0-9a-f]{64}$/.test(chartParam.value)) {
  paramError.value = '譜面 ID は MD5 (32 hex) または SHA256 (64 hex) である必要があります'
}

const sha256 = computed(() =>
  /^[0-9a-f]{64}$/.test(chartParam.value) ? chartParam.value : '',
)
const canLoadChart = computed(() => sha256.value.length === 64)

type LnPolicyFilter = 'ALL' | LnScorePolicy

const lnPolicy = ref<LnPolicyFilter>('ALL')
const lnPolicies: LnPolicyFilter[] = [
  'ALL',
  'AutoLn',
  'AutoCn',
  'AutoHcn',
  'ForceLn',
  'ForceCn',
  'ForceHcn',
]

const { data: detail, error: detailError } = await useFetch<ChartDetail>(
  () => `/api/v1/charts/${sha256.value}`,
  { immediate: canLoadChart.value, watch: [sha256] },
)
const {
  data: ranking,
  pending: rankingPending,
  error: rankingError,
} = await useFetch<IrRanking>(() => `/api/v1/charts/${sha256.value}/ranking`, {
  immediate: canLoadChart.value,
  watch: [sha256, lnPolicy],
  query: computed(() => ({
    scope: 'global',
    ...(lnPolicy.value === 'ALL' ? {} : { ln_policy: lnPolicy.value }),
  })),
})
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <UAlert v-if="paramError" color="error" :description="paramError" class="mb-6" />
      <UAlert v-else-if="detailError" color="error" :description="detailError.message" class="mb-6" />
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
            ・ {{ detail.chart.notes }} notes ・ プレイ {{ detail.stats.global.play_count }} /
            クリア
            {{ detail.stats.global.clear_count }}
          </p>
        </div>

        <div class="mb-4 flex flex-wrap items-center gap-3">
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
                <th class="px-3 py-2">条件</th>
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
                <td class="px-3 py-2 text-right font-medium">
                  <NuxtLink :to="`/scores/${entry.score.score_id}`" class="hover:underline">
                    {{ entry.score.ex_score }}
                  </NuxtLink>
                </td>
                <td class="px-3 py-2">{{ entry.score.clear }}</td>
                <td class="px-3 py-2 text-neutral-400">
                  {{ entry.score.gauge }} / {{ entry.score.ln_policy }}
                </td>
                <td class="px-3 py-2 text-right">{{ entry.score.max_combo }}</td>
                <td class="px-3 py-2 text-right">{{ entry.score.min_bp }}</td>
                <td class="px-3 py-2 text-neutral-400">{{ entry.score.device_type }}</td>
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
