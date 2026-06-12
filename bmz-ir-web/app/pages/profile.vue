<script setup lang="ts">
import type { FormSubmitEvent } from '@nuxt/ui'

type ProfileState = {
  displayName: string
  bio: string
}

type ProfileResponse = {
  player: {
    id: string
    email: string
    display_name: string
    bio: string
  }
}

const state = reactive<ProfileState>({
  displayName: '',
  bio: '',
})

const loading = ref(false)
const saving = ref(false)
const errorMessage = ref('')
const successMessage = ref('')
const requestFetch = useRequestFetch()
const { user, fetch: refreshSession } = useUserSession()

await loadProfile()

function validate(profile: Partial<ProfileState>) {
  const errors: { name: keyof ProfileState; message: string }[] = []

  if (!profile.displayName?.trim()) {
    errors.push({ name: 'displayName', message: '名前を入力してください。' })
  }

  if ((profile.displayName?.trim().length ?? 0) > 64) {
    errors.push({ name: 'displayName', message: '名前は64文字以内にしてください。' })
  }

  if ((profile.bio?.length ?? 0) > 1000) {
    errors.push({ name: 'bio', message: '自己紹介は1000文字以内にしてください。' })
  }

  return errors
}

async function loadProfile() {
  if (!user.value) {
    await navigateTo('/signin')
    return
  }
  loading.value = true
  errorMessage.value = ''

  try {
    const response = await requestFetch<ProfileResponse>('/api/v1/profile')
    state.displayName = response.player.display_name || ''
    state.bio = response.player.bio || ''
  } catch (error) {
    errorMessage.value =
      error instanceof Error ? error.message : 'プロフィールの取得に失敗しました。'
  } finally {
    loading.value = false
  }
}

async function submit(event: FormSubmitEvent<ProfileState>) {
  if (!user.value) {
    await navigateTo('/signin')
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
      body: { display_name: displayName, bio },
    })
    await refreshSession()
    state.displayName = displayName
    state.bio = bio
    successMessage.value = 'プロフィールを保存しました。'
  } catch (error) {
    errorMessage.value =
      error instanceof Error ? error.message : 'プロフィールの保存に失敗しました。'
  } finally {
    saving.value = false
    return
  }
}
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto flex min-h-dvh w-full max-w-2xl flex-col justify-center px-5 py-10">
      <div class="space-y-6">
        <div>
          <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
          <h1 class="text-3xl font-semibold tracking-normal">プロフィール編集</h1>
          <p class="mt-3 text-sm leading-6 text-neutral-300">
            ランキングやユーザーページに表示する情報を編集します。
          </p>
        </div>

        <UForm :state="state" :validate="validate" class="space-y-5" @submit="submit">
          <UFormField label="名前" name="displayName" required>
            <UInput
              v-model="state.displayName"
              autocomplete="nickname"
              :disabled="loading"
              maxlength="64"
              placeholder="name"
              size="xl"
            />
          </UFormField>

          <UFormField label="自己紹介" name="bio">
            <UTextarea
              v-model="state.bio"
              :disabled="loading"
              maxlength="1000"
              placeholder="好きな譜面、プレイスタイルなど"
              :rows="6"
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
              保存する
            </UButton>
            <UButton color="neutral" icon="i-lucide-house" size="xl" to="/" variant="subtle">
              トップへ戻る
            </UButton>
          </div>
        </UForm>
      </div>
    </section>
  </main>
</template>
