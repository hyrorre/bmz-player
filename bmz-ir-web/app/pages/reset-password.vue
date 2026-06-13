<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'

type RequestResetState = {
  email: string
}

const { user } = useUserSession()

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

const requestLoading = ref(false)
const requestErrorMessage = ref('')
const requestSuccessMessage = ref('')

function validateRequest(state: Partial<RequestResetState>) {
  const errors: { name: keyof RequestResetState; message: string }[] = []

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: 'メールアドレスを入力してください。' })
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
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto flex min-h-dvh w-full max-w-md flex-col justify-center px-5 py-10">
      <UCard v-if="user" class="w-full">
        <template #header>
          <div class="flex items-center gap-3">
            <UIcon class="size-5 text-primary-300" name="i-lucide-key-round" />
            <h1 class="text-xl font-semibold">パスワード変更</h1>
          </div>
        </template>

        <p class="text-sm leading-6 text-neutral-300">
          ログイン中のパスワード変更は、現在のパスワード確認が必要です。
        </p>

        <template #footer>
          <UButton block color="primary" to="/settings">アカウント設定へ移動</UButton>
        </template>
      </UCard>

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
            <NuxtLink class="font-medium text-primary-300 hover:text-primary-200" to="/login">
              ログイン
            </NuxtLink>
          </p>
        </template>
      </UAuthForm>
    </section>
  </main>
</template>
