<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'

type RegisterState = {
  displayName: string
  email: string
  password: string
}

const { loggedIn, fetch: refreshSession } = useUserSession()

const fields: AuthFormField[] = [
  {
    name: 'displayName',
    type: 'text',
    label: '表示名',
    placeholder: 'name',
    autocomplete: 'nickname',
    required: true,
    defaultValue: '',
  },
  {
    name: 'email',
    type: 'email',
    label: 'メールアドレス',
    placeholder: 'name@example.com',
    autocomplete: 'email',
    required: true,
    defaultValue: '',
  },
  {
    name: 'password',
    type: 'password',
    label: 'パスワード',
    placeholder: '8文字以上',
    autocomplete: 'new-password',
    required: true,
    defaultValue: '',
  },
]

const loading = ref(false)
const errorMessage = ref('')
const successMessage = ref('')

if (loggedIn.value) {
  await navigateTo('/')
}

function validate(state: Partial<RegisterState>) {
  const errors: { name: keyof RegisterState; message: string }[] = []

  if (!state.displayName?.trim()) {
    errors.push({ name: 'displayName', message: '表示名を入力してください。' })
  }

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: 'メールアドレスを入力してください。' })
  }

  if (!state.password || state.password.length < 8) {
    errors.push({ name: 'password', message: 'パスワードは8文字以上にしてください。' })
  }

  return errors
}

async function submit(event: FormSubmitEvent<RegisterState>) {
  errorMessage.value = ''
  successMessage.value = ''
  loading.value = true

  try {
    await $fetch('/api/v1/auth/register', {
      method: 'POST',
      body: {
        email: event.data.email.trim(),
        password: event.data.password,
        display_name: event.data.displayName.trim(),
      },
    })
    await refreshSession()
  } catch (error) {
    errorMessage.value =
      error instanceof Error && error.message ? error.message : 'アカウント登録に失敗しました。'
    loading.value = false
    return
  }

  loading.value = false
  successMessage.value = ''
  await navigateTo('/')
}
</script>

<template>
  <main>
    <section class="mx-auto flex w-full max-w-md flex-col justify-center px-5 py-10">
      <UAuthForm
        class="w-full"
        description="BMZ IR にスコアを送信するためのアカウントを作成します。"
        :fields="fields"
        icon="i-lucide-user-plus"
        :loading="loading"
        :submit="{ label: '登録する', color: 'primary', block: true }"
        title="アカウント登録"
        :validate="validate"
        @submit="submit"
      >
        <template #validation>
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
        </template>

        <template #footer>
          <p class="text-center text-sm text-neutral-300">
            アカウントをお持ちの場合は
            <NuxtLink class="font-medium text-primary-300 hover:text-primary-200" to="/login">
              ログイン
            </NuxtLink>
          </p>
        </template>
      </UAuthForm>
    </section>
  </main>
</template>
