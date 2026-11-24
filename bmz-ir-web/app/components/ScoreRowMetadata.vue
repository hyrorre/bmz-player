<script setup lang="ts">
import type { ScoreClient } from '../types/scoreDetails'
import { formatScoreClientVersion } from '../utils/rankingDisplay'

const props = defineProps<{
  device: string | null
  date: string | null
  client?: Partial<ScoreClient> | null
}>()
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const clientVersion = computed(() => formatScoreClientVersion(props.client))
</script>

<template>
  <dl class="flex flex-wrap gap-x-6 gap-y-2 text-sm">
    <div>
      <dt class="text-xs text-muted">{{ t('table.input') }}</dt>
      <dd>{{ device ?? '-' }}</dd>
    </div>
    <div>
      <dt class="text-xs text-muted">{{ t('table.date') }}</dt>
      <dd>{{ date ? formatDateTime(date) : '-' }}</dd>
    </div>
    <div v-if="clientVersion" class="min-w-0">
      <dt class="text-xs text-muted">{{ t('score.clientVersion') }}</dt>
      <dd class="wrap-anywhere">{{ clientVersion }}</dd>
    </div>
  </dl>
</template>
