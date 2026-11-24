<script setup lang="ts">
import type { IrJudges } from '../../../shared/types/ir'
import type { ScoreReplay } from '../../types/scoreDetails'

interface ScoreDetail {
  score: {
    id: string
    player_id: string
    chart_sha256: string
    clear_type: string
    ex_score: number
    max_combo: number
    notes: number
    min_bp: number
    min_cb: number
    gauge: string
    ln_policy: string
    rule_mode: string
    arrange_1p?: string | null
    arrange_2p?: string | null
    judges: Partial<IrJudges> | null
    device_type: string
    platform: string
    client_name: string
    client_version: string
    played_at: string | null
    server_received_at: string
    verification: string
    replay_hash: string | null
  }
  player: { id: string; display_name: string }
  chart: {
    sha256: string
    title: string
    subtitle: string | null
    artist: string | null
    mode: string
    level: number | null
    notes: number
  } | null
  replay: ScoreReplay | null
}

const route = useRoute()
const scoreId = computed(() => String(route.params.id ?? ''))
const localePath = useLocalePath()
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const { data, pending, error } = await useFetch<ScoreDetail>(
  () => `/api/v1/scores/${scoreId.value}`,
)

const verificationBadge = computed(() => {
  switch (data.value?.score.verification) {
    case 'verified_play':
      return { color: 'success' as const, label: t('score.verifiedPlay') }
    case 'signed_backfill':
      return { color: 'info' as const, label: t('score.signedBackfill') }
    default:
      return { color: 'neutral' as const, label: t('score.unsigned') }
  }
})

function formatArrange(value: string | null | undefined) {
  return value ? value.toUpperCase() : '-'
}

const arrangeLabel = computed(() => {
  const score = data.value?.score
  return `ARRANGE 1P ${formatArrange(score?.arrange_1p)} / 2P ${formatArrange(score?.arrange_2p)}`
})
const errorDescription = computed(() =>
  error.value ? translateApiError(error.value, 'errors.scoreLoadFailed') : '',
)
useSeoMeta({ title: () => t('score.title') })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-3xl px-5 py-10">
      <UAlert v-if="error" color="error" :description="errorDescription" />
      <p v-else-if="pending" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
      <template v-else-if="data">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink
              :to="localePath({ path: `/charts/${data.score.chart_sha256}`, query: route.query })"
              class="hover:underline"
            >
              {{ data.chart?.title ?? data.score.chart_sha256.slice(0, 12) }}
            </NuxtLink>
            {{ t('score.scoreSuffix') }}
          </p>
          <h1 class="text-3xl font-semibold">
            <NuxtLink :to="localePath(`/players/${data.player.id}`)" class="hover:underline">
              {{ data.player.display_name }}
            </NuxtLink>
          </h1>
          <p class="mt-2 text-sm text-neutral-400">
            {{ data.score.gauge }} / {{ data.score.ln_policy }} / {{ data.score.rule_mode }} ・
            {{ arrangeLabel }} ・ {{ data.score.device_type }} ・
            {{ formatDateTime(data.score.played_at ?? data.score.server_received_at) }}
            <UBadge :color="verificationBadge.color" size="sm" variant="subtle">
              {{ verificationBadge.label }}
            </UBadge>
          </p>
        </div>

        <div class="mb-8 grid grid-cols-2 gap-4 sm:grid-cols-4">
          <div class="rounded-lg border border-neutral-800 p-4">
            <p class="text-xs text-neutral-500">EX SCORE</p>
            <p class="text-2xl font-semibold">{{ data.score.ex_score }}</p>
          </div>
          <div class="rounded-lg border border-neutral-800 p-4">
            <p class="text-xs text-neutral-500">CLEAR</p>
            <p class="text-2xl font-semibold">{{ data.score.clear_type }}</p>
          </div>
          <div class="rounded-lg border border-neutral-800 p-4">
            <p class="text-xs text-neutral-500">MAX COMBO</p>
            <p class="text-xl sm:text-2xl">
              <RankingMaxCombo :combo="data.score.max_combo" :total-notes="data.score.notes" />
            </p>
          </div>
          <div class="flex flex-wrap gap-x-4 gap-y-1 rounded-lg border border-neutral-800 p-4">
            <div>
              <p class="text-xs text-neutral-500">BP</p>
              <p class="text-xl font-semibold tabular-nums sm:text-2xl">{{ data.score.min_bp }}</p>
            </div>
            <div>
              <p class="text-xs text-neutral-500">CB</p>
              <p class="text-xl font-semibold tabular-nums sm:text-2xl">{{ data.score.min_cb }}</p>
            </div>
          </div>
        </div>

        <ScoreAttemptDetails
          :key="data.score.id"
          :score-id="data.score.id"
          :judges="data.score.judges"
          :replay="data.replay"
        />

        <div class="mt-10 text-xs text-neutral-600">
          {{ data.score.client_name }} {{ data.score.client_version }} ({{ data.score.platform }})
          ・ score id {{ data.score.id }}
        </div>
      </template>
    </section>
  </main>
</template>
