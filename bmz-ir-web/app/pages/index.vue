<script setup lang="ts">
type SessionUser = {
  id?: string
  sub?: string
  email?: string
}

const { user } = useUserSession()
const localePath = useLocalePath()
const { t } = useI18n()

useSeoMeta({ title: () => t('home.title') })
</script>

<template>
  <main>
    <section class="mx-auto flex w-full max-w-2xl flex-col justify-center px-5 py-10">
      <div class="space-y-8">
        <div>
          <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
          <h1 class="text-4xl font-semibold tracking-normal">BMZ IR</h1>
          <p class="mt-3 max-w-xl text-sm leading-6 text-neutral-300">
            {{ t('home.description') }}
          </p>
        </div>

        <div v-if="user" class="space-y-4">
          <UAlert
            color="success"
            variant="subtle"
            icon="i-lucide-circle-check"
            :description="t('home.loggedInAs', { name: user.displayName })"
          />
          <div class="flex flex-col gap-3 sm:flex-row">
            <UButton
              color="neutral"
              icon="i-lucide-user-pen"
              size="xl"
              :to="localePath('/profile')"
              variant="subtle"
            >
              {{ t('home.editProfile') }}
            </UButton>
            <UButton
              color="neutral"
              icon="i-lucide-settings"
              size="xl"
              :to="localePath('/settings')"
              variant="subtle"
            >
              {{ t('nav.settings') }}
            </UButton>
            <UButton
              color="neutral"
              icon="i-lucide-log-out"
              size="xl"
              :to="localePath('/logout')"
              variant="subtle"
            >
              {{ t('nav.logout') }}
            </UButton>
          </div>
        </div>

        <div v-else class="flex flex-col gap-3 sm:flex-row">
          <UButton color="primary" icon="i-lucide-log-in" size="xl" :to="localePath('/login')">
            {{ t('nav.login') }}
          </UButton>
          <UButton
            color="neutral"
            icon="i-lucide-user-plus"
            size="xl"
            :to="localePath('/register')"
            variant="subtle"
          >
            {{ t('nav.register') }}
          </UButton>
        </div>

        <div class="flex flex-col gap-3 sm:flex-row">
          <UButton
            color="neutral"
            icon="i-lucide-list-music"
            size="xl"
            :to="localePath('/charts')"
            variant="subtle"
          >
            {{ t('home.chartsRanking') }}
          </UButton>
          <UButton
            color="neutral"
            icon="i-lucide-medal"
            size="xl"
            :to="localePath('/courses')"
            variant="subtle"
          >
            {{ t('home.coursesDan') }}
          </UButton>
          <UButton
            v-if="user"
            color="neutral"
            icon="i-lucide-trophy"
            size="xl"
            :to="localePath(`/players/${(user as SessionUser).sub ?? (user as SessionUser).id}`)"
            variant="subtle"
          >
            {{ t('home.myScores') }}
          </UButton>
          <UButton
            v-if="user"
            color="neutral"
            icon="i-lucide-calendar-days"
            size="xl"
            :to="localePath('/daily')"
            variant="subtle"
          >
            {{ t('nav.daily') }}
          </UButton>
        </div>
      </div>
    </section>
  </main>
</template>
