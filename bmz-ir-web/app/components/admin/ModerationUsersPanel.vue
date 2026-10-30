<script setup lang="ts">
import { USER_MODERATION_STATUSES, type UserModerationStatus } from '#shared/constants/moderation'
import type {
  AdminUserDetail,
  AdminUserListItem,
  ModerationChangeResult,
  Pagination,
} from '../../types/moderation'

const props = defineProps<{ currentUserId: string | null }>()
const { t } = useI18n()
const localePath = useLocalePath()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()
const { notifyResults, notifyError } = useModerationResultToast()

const search = ref('')
const appliedSearch = ref('')
const statusFilter = ref<'all' | UserModerationStatus>('all')
const page = ref(1)
const pageSize = 30
const selectedId = ref<string | null>(null)

const statusItems = computed(() => [
  { label: t('admin.allStatuses'), value: 'all' as const },
  ...USER_MODERATION_STATUSES.map((status) => ({
    label: t(`admin.userStatus.${status}`),
    value: status,
  })),
])

const {
  data: list,
  pending: listPending,
  error: listError,
  refresh: refreshList,
} = await useFetch<{ users: AdminUserListItem[]; pagination: Pagination }>(
  '/api/v1/admin/moderation/users',
  {
    query: computed(() => ({
      limit: pageSize,
      offset: (page.value - 1) * pageSize,
      ...(appliedSearch.value ? { q: appliedSearch.value } : {}),
      ...(statusFilter.value === 'all' ? {} : { status: statusFilter.value }),
    })),
    watch: [page, statusFilter],
  },
)

const {
  data: detail,
  pending: detailPending,
  error: detailError,
  refresh: refreshDetail,
} = await useFetch<AdminUserDetail>(
  () => `/api/v1/admin/moderation/users/${encodeURIComponent(selectedId.value ?? '')}`,
  { immediate: false, watch: false },
)

watch(selectedId, (id) => {
  if (id) refreshDetail()
})
watch(statusFilter, () => {
  page.value = 1
})

function applySearch() {
  appliedSearch.value = search.value.trim()
  if (page.value === 1) refreshList()
  else page.value = 1
}

const restrictionNotice = computed(() => {
  if (!detail.value) return ''
  if (detail.value.user.id === props.currentUserId) return t('admin.users.selfAccount')
  if (detail.value.user.is_admin) return t('admin.users.adminAccount')
  return ''
})

const availableActions = computed(() => {
  if (!detail.value || restrictionNotice.value) {
    // 管理者アカウントでも、誤って制限された状態からの解除は許可する。
    if (detail.value?.user.is_admin && detail.value.user.id !== props.currentUserId) {
      return detail.value.moderation.status === 'active' ? [] : (['active'] as const)
    }
    return []
  }
  return USER_MODERATION_STATUSES.filter((status) => status !== detail.value!.moderation.status)
})

const confirmOpen = ref(false)
const confirmStatus = ref<UserModerationStatus>('active')
const submitting = ref(false)

function openConfirm(status: UserModerationStatus) {
  confirmStatus.value = status
  confirmOpen.value = true
}

async function applyStatus({ reason }: { reason: string }) {
  if (!detail.value) return
  submitting.value = true
  try {
    const result = await $fetch<ModerationChangeResult<UserModerationStatus>>(
      `/api/v1/admin/moderation/users/${encodeURIComponent(detail.value.user.id)}/status`,
      {
        method: 'POST',
        body: {
          status: confirmStatus.value,
          reason,
          expected_revision: detail.value.moderation.revision,
        },
      },
    )
    notifyResults([result])
    confirmOpen.value = false
  } catch (error) {
    notifyError(error)
  } finally {
    submitting.value = false
    await Promise.all([refreshList(), refreshDetail()])
  }
}

function actionColor(status: UserModerationStatus) {
  return status === 'banned' ? 'error' : status === 'submit_suspended' ? 'warning' : 'primary'
}

function formatOptionalDate(value: string | null) {
  return value ? formatDateTime(value) : '-'
}
</script>

<template>
  <div class="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
    <section class="min-w-0">
      <div class="mb-4 flex flex-col gap-2 sm:flex-row">
        <UInput
          v-model="search"
          class="flex-1"
          icon="i-lucide-search"
          :placeholder="t('admin.users.searchPlaceholder')"
          @keydown.enter="applySearch"
        />
        <div class="flex gap-2">
          <USelect v-model="statusFilter" :items="statusItems" class="w-40" />
          <UButton color="primary" variant="subtle" @click="applySearch">
            {{ t('common.search') }}
          </UButton>
        </div>
      </div>

      <UAlert
        v-if="listError"
        color="error"
        :description="translateApiError(listError, 'admin.loadFailed')"
      />
      <p v-else-if="listPending && !list" class="text-sm text-muted">{{ t('common.loading') }}</p>
      <p v-else-if="!list?.users.length" class="text-sm text-muted">{{ t('admin.users.empty') }}</p>
      <ul v-else class="divide-y divide-default rounded-lg border border-default">
        <li v-for="user in list.users" :key="user.id">
          <button
            type="button"
            class="flex w-full items-center justify-between gap-3 px-4 py-3 text-left hover:bg-elevated"
            :class="{ 'bg-elevated': selectedId === user.id }"
            @click="selectedId = user.id"
          >
            <div class="min-w-0">
              <p class="truncate font-medium">{{ user.display_name || user.id }}</p>
              <p class="truncate text-xs text-muted">{{ user.email }}</p>
              <p class="truncate font-mono text-xs text-dimmed">{{ user.id }}</p>
            </div>
            <AdminModerationStatusBadge kind="user" :status="user.status" />
          </button>
        </li>
      </ul>
      <div v-if="list && list.pagination.total > pageSize" class="mt-4 flex justify-end">
        <UPagination
          v-model:page="page"
          :items-per-page="pageSize"
          :total="list.pagination.total"
        />
      </div>
    </section>

    <section class="min-w-0 rounded-xl border border-default p-5">
      <p v-if="!selectedId" class="text-sm text-muted">{{ t('admin.notSelected') }}</p>
      <UAlert
        v-else-if="detailError"
        color="error"
        :description="translateApiError(detailError, 'admin.loadFailed')"
      />
      <p v-else-if="detailPending && !detail" class="text-sm text-muted">
        {{ t('common.loading') }}
      </p>
      <template v-else-if="detail">
        <div class="mb-4 flex flex-wrap items-start justify-between gap-3">
          <div class="min-w-0">
            <h2 class="truncate text-xl font-semibold">
              {{ detail.user.display_name || detail.user.id }}
            </h2>
            <p class="break-all text-sm text-muted">{{ detail.user.email }}</p>
            <p class="break-all font-mono text-xs text-dimmed">{{ detail.user.id }}</p>
          </div>
          <AdminModerationStatusBadge kind="user" :status="detail.moderation.status" />
        </div>

        <dl class="mb-4 grid gap-x-4 gap-y-1 text-sm sm:grid-cols-2">
          <div>
            <dt class="text-muted">{{ t('admin.users.createdAt') }}</dt>
            <dd>{{ formatDateTime(detail.user.created_at) }}</dd>
          </div>
          <div>
            <dt class="text-muted">{{ t('admin.users.lastScore') }}</dt>
            <dd>{{ formatOptionalDate(detail.stats.last_score_at) }}</dd>
          </div>
          <div class="sm:col-span-2">
            <dd>
              {{
                t('admin.users.scores', {
                  total: detail.stats.score_count,
                  accepted: detail.stats.accepted_score_count,
                })
              }}
              /
              {{ t('admin.users.bestScores', { count: detail.stats.best_score_count }) }}
              /
              {{ t('admin.users.courseScores', { count: detail.stats.course_score_count }) }}
              /
              {{ t('admin.users.sessions', { count: detail.stats.active_session_count }) }}
            </dd>
          </div>
        </dl>

        <div v-if="detail.moderation.revision > 0" class="mb-4 rounded-lg bg-elevated p-3 text-sm">
          <p>
            <span class="text-muted">{{ t('admin.reason') }}:</span>
            {{ detail.moderation.reason || '-' }}
          </p>
          <p class="text-muted">
            {{ t('admin.updatedAt') }}: {{ formatOptionalDate(detail.moderation.updated_at) }}
            ·
            {{ t('admin.updatedBy') }}:
            {{
              detail.moderation.updated_by?.display_name || detail.moderation.updated_by?.id || '-'
            }}
          </p>
        </div>

        <UAlert
          v-if="restrictionNotice"
          color="neutral"
          variant="subtle"
          :description="restrictionNotice"
          class="mb-4"
        />
        <div class="mb-6 flex flex-wrap gap-2">
          <UButton
            v-for="status in availableActions"
            :key="status"
            :color="actionColor(status)"
            variant="subtle"
            @click="openConfirm(status)"
          >
            {{ t(`admin.userActions.${status}`) }}
          </UButton>
          <UButton
            color="neutral"
            variant="ghost"
            icon="i-lucide-external-link"
            :to="localePath(`/players/${detail.user.id}`)"
            target="_blank"
          >
            {{ t('admin.users.viewProfile') }}
          </UButton>
        </div>

        <h3 class="mb-2 font-semibold">{{ t('admin.users.latestScores') }}</h3>
        <p v-if="!detail.latest_scores.length" class="text-sm text-muted">
          {{ t('admin.users.noScores') }}
        </p>
        <div v-else class="overflow-x-auto rounded-lg border border-default">
          <table class="w-full text-sm">
            <tbody>
              <tr
                v-for="score in detail.latest_scores"
                :key="score.score_id"
                class="border-t border-default first:border-t-0"
              >
                <td class="px-3 py-2">
                  <p class="max-w-64 truncate">{{ score.title || t('common.untitled') }}</p>
                  <p class="text-xs text-muted">{{ formatDateTime(score.server_received_at) }}</p>
                </td>
                <td class="px-3 py-2 text-right whitespace-nowrap">
                  EX {{ score.ex_score }} · {{ score.clear }}
                </td>
                <td class="px-3 py-2 text-right whitespace-nowrap">
                  <UBadge v-if="!score.accepted" color="neutral" size="sm" variant="outline">
                    {{ t('admin.users.rejected') }}
                  </UBadge>
                  <AdminModerationStatusBadge
                    v-if="score.chart_status !== 'active'"
                    kind="chart"
                    :status="score.chart_status"
                  />
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </section>

    <AdminModerationConfirmModal
      v-model:open="confirmOpen"
      :status-label="t(`admin.userStatus.${confirmStatus}`)"
      :targets="detail ? [`${detail.user.display_name} (${detail.user.id})`] : []"
      :pending="submitting"
      @confirm="applyStatus"
    />
  </div>
</template>
