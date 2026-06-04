<script setup lang="ts">
import type { FormSubmitEvent } from '@nuxt/ui'
import type { User } from '@supabase/supabase-js'
import type { Database } from '~~/bmz-ir-web/shared/types/database.types'

type ProfileState = {
  displayName: string
  bio: string
}

const supabase = useSupabaseClient<Database>()
const currentUser = ref<User | null>(null)

const state = reactive<ProfileState>({
  displayName: '',
  bio: '',
})

const loading = ref(false)
const saving = ref(false)
const errorMessage = ref('')
const successMessage = ref('')

const initialUser = await getAuthenticatedUser()

if (initialUser) {
  await loadProfile(initialUser)
}

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

async function getAuthenticatedUser() {
  const { data, error } = await supabase.auth.getUser()

  if (error || !data.user?.id) {
    currentUser.value = null
    await navigateTo('/signin')
    return null
  }

  currentUser.value = data.user
  return data.user
}

async function loadProfile(profileUser: User) {
  loading.value = true
  errorMessage.value = ''

  const { data, error } = await supabase
    .from('profiles')
    .select('display_name, bio')
    .eq('id', profileUser.id)
    .maybeSingle()

  loading.value = false

  if (error) {
    errorMessage.value = error.message
    return
  }

  state.displayName = data?.display_name || profileUser.user_metadata?.display_name || ''
  state.bio = data?.bio || ''

  if (!data) {
    const { error: insertError } = await supabase.from('profiles').upsert({
      id: profileUser.id,
      display_name: state.displayName,
      bio: state.bio,
    })

    if (insertError) {
      errorMessage.value = insertError.message
    }
  }
}

async function submit(event: FormSubmitEvent<ProfileState>) {
  const profileUser = await getAuthenticatedUser()

  if (!profileUser) {
    return
  }

  errorMessage.value = ''
  successMessage.value = ''
  saving.value = true

  const displayName = event.data.displayName.trim()
  const bio = event.data.bio.trim()

  const { error: profileError } = await supabase.from('profiles').upsert({
    id: profileUser.id,
    display_name: displayName,
    bio,
  })

  if (!profileError) {
    const { error: metadataError } = await supabase.auth.updateUser({
      data: {
        display_name: displayName,
      },
    })

    if (metadataError) {
      errorMessage.value = metadataError.message
      saving.value = false
      return
    }
  }

  saving.value = false

  if (profileError) {
    errorMessage.value = profileError.message
    return
  }

  state.displayName = displayName
  state.bio = bio
  successMessage.value = 'プロフィールを保存しました。'
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
