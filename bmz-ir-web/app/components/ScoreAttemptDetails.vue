<script setup lang="ts">
import type { IrJudges } from '../../shared/types/ir'
import type { ScoreClient, ScoreReplay } from '../types/scoreDetails'
import { formatScoreClientVersion } from '../utils/rankingDisplay'

const props = defineProps<{
  scoreId: string
  judges?: Partial<IrJudges> | null
  replay?: ScoreReplay | null
  client?: ScoreClient | null
}>()
const { t } = useI18n()
const { formatNumber } = useLocaleFormat()
const { translateApiError } = useApiError()
const downloading = ref(false)
const replayError = ref('')

const judgeLabels = {
  pgreat: 'PGREAT',
  great: 'GREAT',
  good: 'GOOD',
  bad: 'BAD',
  poor: 'POOR',
  empty_poor: 'EMPTY POOR',
} as const

function displayCount(count: number | undefined) {
  return typeof count === 'number' && Number.isFinite(count) ? formatNumber(count) : '–'
}

const judgeRows = computed(() =>
  (Object.keys(judgeLabels) as (keyof typeof judgeLabels)[]).map((key) => {
    const fast = props.judges?.fast?.[key]
    const slow = props.judges?.slow?.[key]
    return {
      key,
      label: judgeLabels[key],
      fast: displayCount(fast),
      slow: displayCount(slow),
      total: displayCount(
        typeof fast === 'number' && typeof slow === 'number' ? fast + slow : undefined,
      ),
    }
  }),
)
const replayAvailable = computed(() =>
  ['uploaded', 'verified'].includes(props.replay?.status ?? ''),
)
const clientVersion = computed(() => formatScoreClientVersion(props.client))

async function downloadReplay() {
  if (downloading.value) return
  const scoreId = props.scoreId
  replayError.value = ''
  downloading.value = true
  try {
    const replay = await $fetch<Blob>(`/api/v1/scores/${encodeURIComponent(scoreId)}/replay/raw`, {
      responseType: 'blob',
    })
    const url = URL.createObjectURL(replay)
    const link = document.createElement('a')
    link.href = url
    link.download = `replay-${scoreId}.bin`
    document.body.append(link)
    link.click()
    link.remove()
    window.setTimeout(() => URL.revokeObjectURL(url), 1000)
  } catch (error) {
    replayError.value = translateApiError(error, 'errors.replayLoadFailed')
  } finally {
    downloading.value = false
  }
}
</script>

<template>
  <div class="grid min-w-0 gap-4 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
    <section class="min-w-0 space-y-2">
      <h3 class="font-semibold text-highlighted">{{ t('score.judgementBreakdown') }}</h3>
      <div class="overflow-x-auto rounded-lg border border-default bg-default">
        <table class="w-full text-sm tabular-nums">
          <thead class="bg-elevated text-left text-toned">
            <tr>
              <th scope="col" class="px-2 py-1.5 sm:px-3">{{ t('score.judgement') }}</th>
              <th scope="col" class="px-2 py-1.5 text-right sm:px-3">FAST</th>
              <th scope="col" class="px-2 py-1.5 text-right sm:px-3">SLOW</th>
              <th scope="col" class="px-2 py-1.5 text-right sm:px-3">{{ t('table.total') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in judgeRows" :key="row.key" class="border-t border-default">
              <th scope="row" class="px-2 py-1.5 text-left font-normal sm:px-3">{{ row.label }}</th>
              <td class="px-2 py-1.5 text-right sm:px-3">{{ row.fast }}</td>
              <td class="px-2 py-1.5 text-right sm:px-3">{{ row.slow }}</td>
              <td class="px-2 py-1.5 text-right font-medium sm:px-3">{{ row.total }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
    <section class="min-w-0 space-y-2">
      <h3 class="font-semibold text-highlighted">{{ t('score.replay') }}</h3>
      <template v-if="replayAvailable && replay">
        <p class="text-sm text-muted">
          {{ replay.format }}
          <template v-if="replay.size_bytes != null">
            · {{ formatNumber(replay.size_bytes) }} bytes</template
          >
          · {{ replay.status === 'verified' ? t('score.hashVerified') : t('score.uploaded') }}
        </p>
        <UButton
          color="primary"
          icon="i-lucide-download"
          :loading="downloading"
          @click="downloadReplay"
        >
          {{ t('score.downloadReplay') }}
        </UButton>
        <UAlert v-if="replayError" role="alert" color="error" :description="replayError" />
      </template>
      <p v-else class="text-sm text-muted">{{ t('score.noReplay') }}</p>
      <p v-if="clientVersion" class="text-xs wrap-anywhere text-muted">{{ clientVersion }}</p>
    </section>
  </div>
</template>
