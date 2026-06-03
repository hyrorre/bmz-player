<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'
import type { Database } from '~~/bmz-ir-web/shared/types/database.types'

type SigninState = {
  email: string
  password: string
}

const supabase = useSupabaseClient<Database>()
const user = useSupabaseUser()

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

if (user.value) {
  await navigateTo('/')
}

function validate(state: Partial<SigninState>) {
  const errors: { name: keyof SigninState; message: string }[] = []

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: 'メールアドレスを入力してください。' })
  }

  if (!state.password) {
    errors.push({ name: 'password', message: 'パスワードを入力してください。' })
  }

  return errors
}

async function submit(event: FormSubmitEvent<SigninState>) {
  errorMessage.value = ''
  loading.value = true

  const { error } = await supabase.auth.signInWithPassword({
    email: event.data.email.trim(),
    password: event.data.password,
  })

  loading.value = false

  if (error) {
    errorMessage.value = error.message
    return
  }

  await navigateTo('/')
}
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto flex min-h-dvh w-full max-w-md flex-col justify-center px-5 py-10">
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
              <NuxtLink class="font-medium text-primary-300 hover:text-primary-200" to="/signup">
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
