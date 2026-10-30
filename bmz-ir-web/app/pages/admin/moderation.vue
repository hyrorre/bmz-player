<script setup lang="ts">
const { t } = useI18n()
const { user } = useUserSession()
const { account, isAdmin } = useIrAccount()
const tab = ref<'users' | 'charts' | 'audit'>('users')

const tabItems = computed(() => [
  { label: t('admin.tabs.users'), value: 'users' as const, icon: 'i-lucide-users' },
  { label: t('admin.tabs.charts'), value: 'charts' as const, icon: 'i-lucide-list-music' },
  { label: t('admin.tabs.audit'), value: 'audit' as const, icon: 'i-lucide-history' },
])

useSeoMeta({ title: () => t('admin.title'), robots: 'noindex, nofollow' })
</script>

<template>
  <main>
    <section class="mx-auto w-full max-w-7xl px-4 py-10 sm:px-5">
      <div class="mb-6">
        <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
        <h1 class="text-3xl font-semibold">{{ t('admin.title') }}</h1>
        <p class="mt-2 text-sm text-neutral-300">{{ t('admin.description') }}</p>
      </div>

      <!-- 表示の出し分けのみ。権限は管理者APIがサーバー側で検証する。 -->
      <UAlert
        v-if="!user || (account && !isAdmin)"
        color="error"
        icon="i-lucide-shield-alert"
        :description="t('admin.forbidden')"
      />
      <p v-else-if="!account" class="text-sm text-muted">{{ t('common.loading') }}</p>
      <template v-else>
        <UTabs v-model="tab" :items="tabItems" :content="false" class="mb-6" />
        <AdminModerationUsersPanel v-if="tab === 'users'" :current-user-id="account.id" />
        <AdminModerationChartsPanel v-else-if="tab === 'charts'" />
        <AdminModerationAuditPanel v-else />
      </template>
    </section>
  </main>
</template>
