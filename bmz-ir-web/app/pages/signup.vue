<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'
import type { Database } from '~~/bmz-ir-web/shared/types/database.types'

type SignupState = {
  displayName: string
  email: string
  password: string
  confirmPassword: string
}

const supabase = useSupabaseClient<Database>()
const user = useSupabaseUser()

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
  {
    name: 'confirmPassword',
    type: 'password',
    label: 'パスワード確認',
    placeholder: 'もう一度入力',
    autocomplete: 'new-password',
    required: true,
    defaultValue: '',
  },
]

const loading = ref(false)
const errorMessage = ref('')
const successMessage = ref('')

if (user.value) {
  await navigateTo('/')
}

function validate(state: Partial<SignupState>) {
  const errors: { name: keyof SignupState; message: string }[] = []

  if (!state.displayName?.trim()) {
    errors.push({ name: 'displayName', message: '表示名を入力してください。' })
  }

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: 'メールアドレスを入力してください。' })
  }

  if (!state.password || state.password.length < 8) {
    errors.push({ name: 'password', message: 'パスワードは8文字以上にしてください。' })
  }

  if (state.password !== state.confirmPassword) {
    errors.push({ name: 'confirmPassword', message: 'パスワードが一致していません。' })
  }

  return errors
}

async function submit(event: FormSubmitEvent<SignupState>) {
  errorMessage.value = ''
  successMessage.value = ''
  loading.value = true

  const { data, error } = await supabase.auth.signUp({
    email: event.data.email.trim(),
    password: event.data.password,
    options: {
      data: {
        display_name: event.data.displayName.trim(),
      },
    },
  })

  loading.value = false

  if (error) {
    errorMessage.value = error.message
    return
  }

  if (data.session) {
    await navigateTo('/')
    return
  }

  successMessage.value = data.user?.identities?.length
    ? '確認メールを送信しました。メール内のリンクから登録を完了してください。'
    : '登録済みのメールアドレスです。メールを確認するか、ログインしてください。'
}
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto flex min-h-dvh w-full max-w-md flex-col justify-center px-5 py-10">
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
            <NuxtLink class="font-medium text-primary-300 hover:text-primary-200" to="/signin">
              ログイン
            </NuxtLink>
          </p>
        </template>
      </UAuthForm>
    </section>
  </main>
</template>
