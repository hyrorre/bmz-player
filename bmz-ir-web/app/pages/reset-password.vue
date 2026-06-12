<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'

type RequestResetState = {
  email: string
}

type ResetPasswordState = {
  password: string
  confirmPassword: string
}

const { user } = useUserSession()
const requestFetch = useRequestFetch()

const requestFields: AuthFormField[] = [
  {
    name: 'email',
    type: 'email',
    label: 'メールアドレス',
    placeholder: 'name@example.com',
    autocomplete: 'email',
    required: true,
    defaultValue: '',
  },
]

const resetFields: AuthFormField[] = [
  {
    name: 'password',
    type: 'password',
    label: '新しいパスワード',
    placeholder: '8文字以上',
    autocomplete: 'new-password',
    required: true,
    defaultValue: '',
  },
  {
    name: 'confirmPassword',
    type: 'password',
    label: '新しいパスワード確認',
    placeholder: 'もう一度入力',
    autocomplete: 'new-password',
    required: true,
    defaultValue: '',
  },
]

const requestLoading = ref(false)
const resetLoading = ref(false)
const requestErrorMessage = ref('')
const requestSuccessMessage = ref('')
const resetErrorMessage = ref('')
const resetSuccessMessage = ref('')

function validateRequest(state: Partial<RequestResetState>) {
  const errors: { name: keyof RequestResetState; message: string }[] = []

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: 'メールアドレスを入力してください。' })
  }

  return errors
}

function validateReset(state: Partial<ResetPasswordState>) {
  const errors: { name: keyof ResetPasswordState; message: string }[] = []

  if (!state.password || state.password.length < 8) {
    errors.push({ name: 'password', message: 'パスワードは8文字以上にしてください。' })
  }

  if (state.password !== state.confirmPassword) {
    errors.push({ name: 'confirmPassword', message: 'パスワードが一致していません。' })
  }

  return errors
}

async function requestReset(event: FormSubmitEvent<RequestResetState>) {
  void event
  requestErrorMessage.value = ''
  requestSuccessMessage.value = ''
  requestLoading.value = true

  requestLoading.value = false
  requestErrorMessage.value =
    'メール送信による再設定は現在未対応です。ログイン中のアカウント設定から変更してください。'
}

async function resetPassword(event: FormSubmitEvent<ResetPasswordState>) {
  resetErrorMessage.value = ''
  resetSuccessMessage.value = ''
  resetLoading.value = true

  try {
    await requestFetch('/api/v1/account/password', {
      method: 'PUT',
      body: { password: event.data.password },
    })
    resetSuccessMessage.value = 'パスワードを再設定しました。'
  } catch (error) {
    resetErrorMessage.value =
      error instanceof Error ? error.message : 'パスワードの再設定に失敗しました。'
  } finally {
    resetLoading.value = false
  }
}
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto flex min-h-dvh w-full max-w-md flex-col justify-center px-5 py-10">
      <UAuthForm
        v-if="user"
        class="w-full"
        description="新しいパスワードを設定します。"
        :fields="resetFields"
        icon="i-lucide-key-round"
        :loading="resetLoading"
        :submit="{ label: 'パスワードを再設定', color: 'primary', block: true }"
        title="パスワード再設定"
        :validate="validateReset"
        @submit="resetPassword"
      >
        <template #validation>
          <UAlert
            v-if="resetErrorMessage"
            color="error"
            icon="i-lucide-circle-alert"
            :description="resetErrorMessage"
          />
          <UAlert
            v-if="resetSuccessMessage"
            color="success"
            icon="i-lucide-circle-check"
            :description="resetSuccessMessage"
          />
        </template>

        <template #footer>
          <p class="text-center text-sm text-neutral-300">
            変更後は
            <NuxtLink class="font-medium text-primary-300 hover:text-primary-200" to="/signin">
              ログイン
            </NuxtLink>
            から再度確認できます
          </p>
        </template>
      </UAuthForm>

      <UAuthForm
        v-else
        class="w-full"
        description="登録メールアドレスへ再設定リンクを送信します。"
        :fields="requestFields"
        icon="i-lucide-mail"
        :loading="requestLoading"
        :submit="{ label: '再設定メールを送信', color: 'primary', block: true }"
        title="パスワードを忘れた場合"
        :validate="validateRequest"
        @submit="requestReset"
      >
        <template #validation>
          <UAlert
            v-if="requestErrorMessage"
            color="error"
            icon="i-lucide-circle-alert"
            :description="requestErrorMessage"
          />
          <UAlert
            v-if="requestSuccessMessage"
            color="success"
            icon="i-lucide-circle-check"
            :description="requestSuccessMessage"
          />
        </template>

        <template #footer>
          <p class="text-center text-sm text-neutral-300">
            パスワードを思い出した場合は
            <NuxtLink class="font-medium text-primary-300 hover:text-primary-200" to="/signin">
              ログイン
            </NuxtLink>
          </p>
        </template>
      </UAuthForm>
    </section>
  </main>
</template>
