<script setup lang="ts">
interface PlayerDetail {
  player: {
    id: string
    display_name: string
    bio: string | null
  }
  best_scores: {
    score_id: string
    chart_sha256: string
    ex_score: number
    clear_type: string
    max_combo: number
    min_bp: number
    min_cb: number
    device_type: string
    gauge: string
    ln_policy: string
    rule_mode: string
    played_at: string | null
    server_received_at: string
    chart: {
      sha256: string
      title: string
      artist: string | null
      mode: string
      level: number | null
    } | null
  }[]
}

const route = useRoute()
const playerId = computed(() => String(route.params.id ?? ''))
const localePath = useLocalePath()
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const { data, pending, error } = await useFetch<PlayerDetail>(
  () => `/api/v1/players/${playerId.value}`,
)
const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.playerLoadFailed') : '',
)
useSeoMeta({ title: () => data.value?.player.display_name ?? t('players.player') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-4xl px-5 py-10">
      <UAlert v-if="error" color="error" :description="errorDescription" />
      <p v-else-if="pending" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
      <template v-else-if="data">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink :to="localePath('/charts')" class="hover:underline">{{
              t('charts.title')
            }}</NuxtLink>
          </p>
          <h1 class="text-3xl font-semibold">{{ data.player.display_name }}</h1>
          <p v-if="data.player.bio" class="mt-2 whitespace-pre-line text-sm text-neutral-300">
            {{ data.player.bio }}
          </p>
          <UButton
            class="mt-4"
            color="neutral"
            icon="i-lucide-calendar-days"
            :to="{ path: localePath('/daily'), query: { player: data.player.id, mode: 'all' } }"
            variant="subtle"
          >
            {{ t('nav.daily') }}
          </UButton>
        </div>

        <h2 class="mb-3 text-lg font-medium">{{ t('players.bestScores') }}</h2>
        <p v-if="!data.best_scores.length" class="text-sm text-neutral-400">
          {{ t('players.noScoresYet') }}
        </p>
        <div v-else class="overflow-x-auto rounded-lg border border-neutral-800">
          <table class="w-full text-sm">
            <thead class="bg-neutral-900 text-left text-neutral-300">
              <tr>
                <th class="px-3 py-2">{{ t('table.chart') }}</th>
                <th class="px-3 py-2 text-right">EX</th>
                <th class="px-3 py-2">{{ t('table.clear') }}</th>
                <th class="px-3 py-2 text-right">COMBO</th>
                <th class="px-3 py-2 text-right">BP</th>
                <th class="px-3 py-2">GAUGE / LN</th>
                <th class="px-3 py-2">{{ t('table.date') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="score in data.best_scores"
                :key="`${score.chart_sha256}-${score.ln_policy}-${score.rule_mode}`"
                class="border-t border-neutral-800"
              >
                <td class="max-w-64 px-3 py-2">
                  <NuxtLink
                    :to="localePath(`/charts/${score.chart_sha256}`)"
                    class="block truncate hover:underline"
                  >
                    {{ score.chart?.title ?? score.chart_sha256.slice(0, 12) }}
                  </NuxtLink>
                </td>
                <td class="px-3 py-2 text-right font-medium">
                  <NuxtLink :to="localePath(`/scores/${score.score_id}`)" class="hover:underline">
                    {{ score.ex_score }}
                  </NuxtLink>
                </td>
                <td class="px-3 py-2">{{ score.clear_type }}</td>
                <td class="px-3 py-2 text-right">{{ score.max_combo }}</td>
                <td class="px-3 py-2 text-right">{{ score.min_bp }}</td>
                <td class="px-3 py-2 text-neutral-400">
                  {{ score.gauge }} / {{ score.ln_policy }} / {{ score.rule_mode }}
                </td>
                <td class="px-3 py-2 text-neutral-400">
                  {{ formatDateTime(score.played_at ?? score.server_received_at) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </section>
  </main>
</template>
