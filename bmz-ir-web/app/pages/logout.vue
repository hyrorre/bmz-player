<script setup lang="ts">
type SessionUser = {
  email?: string
}

const { user, clear } = useUserSession()
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()

const loading = ref(false)
const errorMessage = ref('')

async function logout() {
  errorMessage.value = ''
  loading.value = true

  try {
    await $fetch('/api/v1/auth/logout', { method: 'POST' })
    await clear()
  } catch (error) {
    errorMessage.value = translateApiError(error, 'errors.logoutFailed')
    loading.value = false
    return
  }

  loading.value = false
  await navigateTo(localePath('/login'))
}

useSeoMeta({ title: () => t('nav.logout') })
</script>

<template>
  <main>
    <section class="mx-auto flex w-full max-w-md flex-col justify-center px-5 py-10">
      <div class="space-y-6">
        <div>
          <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
          <h1 class="text-3xl font-semibold tracking-normal">{{ t('nav.logout') }}</h1>
          <p class="mt-3 text-sm leading-6 text-neutral-300">
            {{
              user
                ? t('logout.description', {
                    email: (user as SessionUser).email ?? t('logout.currentUser'),
                  })
                : t('logout.notLoggedIn')
            }}
          </p>
        </div>

        <UAlert
          v-if="errorMessage"
          color="error"
          icon="i-lucide-circle-alert"
          :description="errorMessage"
        />

        <div class="flex flex-col gap-3 sm:flex-row">
          <UButton
            color="primary"
            :disabled="!user"
            icon="i-lucide-log-out"
            :loading="loading"
            size="xl"
            type="button"
            @click="logout"
          >
            {{ t('nav.logout') }}
          </UButton>
          <UButton
            color="neutral"
            icon="i-lucide-house"
            size="xl"
            :to="localePath('/')"
            variant="subtle"
          >
            {{ t('common.backHome') }}
          </UButton>
        </div>
      </div>
    </section>
  </main>
</template>
