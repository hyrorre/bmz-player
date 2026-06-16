<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'

type LoginState = {
  email: string
  password: string
}

const { loggedIn, fetch: refreshSession } = useUserSession()

const fields: AuthFormField[] = [
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
    placeholder: 'パスワード',
    autocomplete: 'current-password',
    required: true,
    defaultValue: '',
  },
]

const loading = ref(false)
const errorMessage = ref('')

if (loggedIn.value) {
  await navigateTo('/')
}

function validate(state: Partial<LoginState>) {
  const errors: { name: keyof LoginState; message: string }[] = []

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: 'メールアドレスを入力してください。' })
  }

  if (!state.password) {
    errors.push({ name: 'password', message: 'パスワードを入力してください。' })
  }

  return errors
}

async function submit(event: FormSubmitEvent<LoginState>) {
  errorMessage.value = ''
  loading.value = true

  try {
    await $fetch('/api/v1/auth/login', {
      method: 'POST',
      body: {
        email: event.data.email.trim(),
        password: event.data.password,
      },
    })
    await refreshSession()
  } catch (error) {
    errorMessage.value =
      error instanceof Error && error.message ? error.message : 'ログインに失敗しました。'
    loading.value = false
    return
  }

  loading.value = false
  await navigateTo('/')
}
</script>

<template>
  <main>
    <section class="mx-auto flex w-full max-w-md flex-col justify-center px-5 py-10">
      <UAuthForm
        class="w-full"
        description="BMZ IR のスコア送信とランキング閲覧に使うアカウントへログインします。"
        :fields="fields"
        icon="i-lucide-log-in"
        :loading="loading"
        :submit="{ label: 'ログイン', color: 'primary', block: true }"
        title="ログイン"
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
        </template>

        <template #footer>
          <div class="space-y-2 text-center text-sm text-neutral-300">
            <p>
              アカウントをお持ちでない場合は
              <NuxtLink class="font-medium text-primary-300 hover:text-primary-200" to="/register">
                登録
              </NuxtLink>
            </p>
            <p>
              パスワードを忘れた場合は
              <NuxtLink
                class="font-medium text-primary-300 hover:text-primary-200"
                to="/reset-password"
              >
                再設定
              </NuxtLink>
            </p>
          </div>
        </template>
      </UAuthForm>
    </section>
  </main>
</template>
