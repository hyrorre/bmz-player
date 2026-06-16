<script setup lang="ts">
interface JudgeCounts {
  pgreat: number
  great: number
  good: number
  bad: number
  poor: number
  empty_poor: number
}

interface ScoreDetail {
  score: {
    id: string
    player_id: string
    chart_sha256: string
    clear_type: string
    ex_score: number
    max_combo: number
    min_bp: number
    bp: number
    cb: number
    gauge: string
    ln_policy: string
    judges: { fast: JudgeCounts; slow: JudgeCounts }
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
  replay: { status: string; size_bytes: number | null; format: string } | null
}

const route = useRoute()
const scoreId = computed(() => String(route.params.id ?? ''))
const { data, pending, error } = await useFetch<ScoreDetail>(
  () => `/api/v1/scores/${scoreId.value}`,
)

const replayAvailable = computed(() =>
  ['uploaded', 'verified'].includes(data.value?.replay?.status ?? ''),
)
const replayError = ref('')

async function downloadReplay() {
  replayError.value = ''
  try {
    const target = await $fetch<{ download_url: string }>(`/api/v1/scores/${scoreId.value}/replay`)
    window.location.href = target.download_url
  } catch (error) {
    replayError.value = error instanceof Error ? error.message : 'リプレイの取得に失敗しました。'
  }
}

function judgeRow(key: keyof JudgeCounts) {
  const judges = data.value?.score.judges
  if (!judges) {
    return { fast: 0, slow: 0, total: 0 }
  }
  return {
    fast: judges.fast[key],
    slow: judges.slow[key],
    total: judges.fast[key] + judges.slow[key],
  }
}

const judgeRows = [
  { key: 'pgreat' as const, label: 'PGREAT' },
  { key: 'great' as const, label: 'GREAT' },
  { key: 'good' as const, label: 'GOOD' },
  { key: 'bad' as const, label: 'BAD' },
  { key: 'poor' as const, label: 'POOR' },
  { key: 'empty_poor' as const, label: 'EMPTY POOR' },
]

const verificationBadge = computed(() => {
  switch (data.value?.score.verification) {
    case 'signed':
      return { color: 'success' as const, label: '署名済み' }
    case 'invalid':
      return { color: 'error' as const, label: '署名不正' }
    case 'trusted':
      return { color: 'success' as const, label: '検証済み' }
    default:
      return { color: 'neutral' as const, label: '未署名' }
  }
})
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-3xl px-5 py-10">
      <UAlert v-if="error" color="error" :description="error.message" />
      <p v-else-if="pending" class="text-sm text-neutral-400">読み込み中...</p>
      <template v-else-if="data">
        <div class="mb-8">
          <p class="mb-2 text-sm font-medium text-primary-300">
            <NuxtLink :to="`/charts/${data.score.chart_sha256}`" class="hover:underline">
              {{ data.chart?.title ?? data.score.chart_sha256.slice(0, 12) }}
            </NuxtLink>
            のスコア
          </p>
          <h1 class="text-3xl font-semibold">
            <NuxtLink :to="`/players/${data.player.id}`" class="hover:underline">
              {{ data.player.display_name }}
            </NuxtLink>
          </h1>
          <p class="mt-2 text-sm text-neutral-400">
            {{ data.score.gauge }} / {{ data.score.ln_policy }} ・ {{ data.score.device_type }} ・
            {{ new Date(data.score.played_at ?? data.score.server_received_at).toLocaleString() }}
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
            <p class="text-2xl font-semibold">{{ data.score.max_combo }}</p>
          </div>
          <div class="rounded-lg border border-neutral-800 p-4">
            <p class="text-xs text-neutral-500">BP</p>
            <p class="text-2xl font-semibold">{{ data.score.bp }}</p>
          </div>
        </div>

        <h2 class="mb-3 text-lg font-medium">判定内訳</h2>
        <div class="mb-8 overflow-x-auto rounded-lg border border-neutral-800">
          <table class="w-full text-sm">
            <thead class="bg-neutral-900 text-left text-neutral-300">
              <tr>
                <th class="px-3 py-2">判定</th>
                <th class="px-3 py-2 text-right">FAST</th>
                <th class="px-3 py-2 text-right">SLOW</th>
                <th class="px-3 py-2 text-right">合計</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in judgeRows" :key="row.key" class="border-t border-neutral-800">
                <td class="px-3 py-2">{{ row.label }}</td>
                <td class="px-3 py-2 text-right">{{ judgeRow(row.key).fast }}</td>
                <td class="px-3 py-2 text-right">{{ judgeRow(row.key).slow }}</td>
                <td class="px-3 py-2 text-right font-medium">{{ judgeRow(row.key).total }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <h2 class="mb-3 text-lg font-medium">リプレイ</h2>
        <div class="space-y-3">
          <UAlert v-if="replayError" color="error" :description="replayError" />
          <template v-if="replayAvailable">
            <p class="text-sm text-neutral-400">
              {{ data.replay?.format }} ・ {{ data.replay?.size_bytes ?? '?' }} bytes ・
              {{ data.replay?.status === 'verified' ? 'hash 検証済み' : 'アップロード済み' }}
            </p>
            <UButton color="primary" icon="i-lucide-download" @click="downloadReplay">
              リプレイをダウンロード
            </UButton>
          </template>
          <p v-else class="text-sm text-neutral-400">
            このスコアのリプレイはアップロードされていません。
          </p>
        </div>

        <div class="mt-10 text-xs text-neutral-600">
          {{ data.score.client_name }} {{ data.score.client_version }} ({{ data.score.platform }})
          ・ score id {{ data.score.id }}
        </div>
      </template>
    </section>
  </main>
</template>
