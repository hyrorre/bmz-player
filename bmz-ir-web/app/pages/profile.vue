<script setup lang="ts">
import type { FormSubmitEvent } from '@nuxt/ui'

type ProfileState = {
  displayName: string
  bio: string
  dailyBoundaryMinutes: number
}

type ProfileResponse = {
  player: {
    id: string
    email: string
    display_name: string
    bio: string
    daily_boundary_minutes: number
  }
}

const state = reactive<ProfileState>({
  displayName: '',
  bio: '',
  dailyBoundaryMinutes: 0,
})

const dailyBoundaryItems = Array.from({ length: 48 }, (_, index) => {
  const value = index * 30
  const hours = Math.floor(value / 60)
  const minutes = value % 60
  return {
    label: `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')} (JST)`,
    value,
  }
})

const loading = ref(false)
const saving = ref(false)
const errorMessage = ref('')
const successMessage = ref('')
const requestFetch = useRequestFetch()
const { user, fetch: refreshSession } = useUserSession()
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()

await loadProfile()

function validate(profile: Partial<ProfileState>) {
  const errors: { name: keyof ProfileState; message: string }[] = []

  if (!profile.displayName?.trim()) {
    errors.push({ name: 'displayName', message: t('validation.displayNameRequired') })
  }

  if ((profile.displayName?.trim().length ?? 0) > 64) {
    errors.push({ name: 'displayName', message: t('validation.displayNameMax') })
  }

  if ((profile.bio?.length ?? 0) > 1000) {
    errors.push({ name: 'bio', message: t('validation.bioMax') })
  }

  return errors
}

async function loadProfile() {
  if (!user.value) {
    await navigateTo(localePath('/login'))
    return
  }
  loading.value = true
  errorMessage.value = ''

  try {
    const response = await requestFetch<ProfileResponse>('/api/v1/profile')
    state.displayName = response.player.display_name || ''
    state.bio = response.player.bio || ''
    state.dailyBoundaryMinutes = response.player.daily_boundary_minutes ?? 0
  } catch (error) {
    errorMessage.value = translateApiError(error, 'errors.profileLoadFailed')
  } finally {
    loading.value = false
  }
}

async function submit(event: FormSubmitEvent<ProfileState>) {
  if (!user.value) {
    await navigateTo(localePath('/login'))
    return
  }

  errorMessage.value = ''
  successMessage.value = ''
  saving.value = true

  const displayName = event.data.displayName.trim()
  const bio = event.data.bio.trim()

  try {
    await requestFetch('/api/v1/profile', {
      method: 'PUT',
      body: {
        display_name: displayName,
        bio,
        daily_boundary_minutes: event.data.dailyBoundaryMinutes,
      },
    })
    await refreshSession()
    state.displayName = displayName
    state.bio = bio
    successMessage.value = t('profile.saved')
  } catch (error) {
    errorMessage.value = translateApiError(error, 'errors.profileSaveFailed')
  } finally {
    saving.value = false
  }
}

useSeoMeta({ title: () => t('profile.title') })
</script>

<template>
  <main>
    <section class="mx-auto flex w-full max-w-2xl flex-col justify-center px-5 py-10">
      <div class="space-y-6">
        <div>
          <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
          <h1 class="text-3xl font-semibold tracking-normal">{{ t('profile.title') }}</h1>
          <p class="mt-3 text-sm leading-6 text-neutral-300">
            {{ t('profile.description') }}
          </p>
        </div>

        <UForm :state="state" :validate="validate" class="space-y-5" @submit="submit">
          <UFormField :label="t('profile.name')" name="displayName" required>
            <UInput
              v-model="state.displayName"
              autocomplete="nickname"
              class="w-full"
              :disabled="loading"
              maxlength="64"
              placeholder="name"
              size="xl"
            />
          </UFormField>

          <UFormField :label="t('profile.bio')" name="bio">
            <UTextarea
              v-model="state.bio"
              class="w-full"
              :disabled="loading"
              maxlength="1000"
              :placeholder="t('profile.bioPlaceholder')"
              :rows="6"
              size="xl"
            />
          </UFormField>

          <UFormField
            :label="t('profile.dailyBoundary')"
            name="dailyBoundaryMinutes"
            :description="t('profile.dailyBoundaryDescription')"
          >
            <USelect
              v-model="state.dailyBoundaryMinutes"
              class="w-full"
              :disabled="loading"
              :items="dailyBoundaryItems"
              size="xl"
            />
          </UFormField>

          <UAlert
            v-if="errorMessage"
            color="error"
            icon="i-lucide-circle-alert"
            :description="errorMessage"
          />
          <UAlert
            v-if="successMessage"
            color="success"
            icon="i-lucide-circle-check"
            :description="successMessage"
          />

          <div class="flex flex-col gap-3 sm:flex-row">
            <UButton color="primary" icon="i-lucide-save" :loading="saving" size="xl" type="submit">
              {{ t('common.save') }}
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
        </UForm>
      </div>
    </section>
  </main>
</template>
