<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'
import type { User } from '@supabase/supabase-js'
import type { Database } from '~~/bmz-ir-web/shared/types/database.types'

type EmailState = {
  email: string
}

type PasswordState = {
  password: string
  confirmPassword: string
}

const supabase = useSupabaseClient<Database>()
const currentUser = ref<User | null>(null)

const emailFields = computed<AuthFormField[]>(() => [
  {
    name: 'email',
    type: 'email',
    label: '新しいメールアドレス',
    placeholder: 'name@example.com',
    autocomplete: 'email',
    required: true,
    defaultValue: currentUser.value?.email ?? '',
  },
])

const passwordFields: AuthFormField[] = [
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

const emailLoading = ref(false)
const passwordLoading = ref(false)
const emailErrorMessage = ref('')
const emailSuccessMessage = ref('')
const passwordErrorMessage = ref('')
const passwordSuccessMessage = ref('')

await requireAuthenticatedUser()

async function requireAuthenticatedUser() {
  const { data, error } = await supabase.auth.getUser()

  if (error || !data.user?.id) {
    currentUser.value = null
    await navigateTo('/signin')
    return null
  }

  currentUser.value = data.user
  return data.user
}

function validateEmail(state: Partial<EmailState>) {
  const errors: { name: keyof EmailState; message: string }[] = []

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: 'メールアドレスを入力してください。' })
  }

  return errors
}

function validatePassword(state: Partial<PasswordState>) {
  const errors: { name: keyof PasswordState; message: string }[] = []

  if (!state.password || state.password.length < 8) {
    errors.push({ name: 'password', message: 'パスワードは8文字以上にしてください。' })
  }

  if (state.password !== state.confirmPassword) {
    errors.push({ name: 'confirmPassword', message: 'パスワードが一致していません。' })
  }

  return errors
}

async function updateEmail(event: FormSubmitEvent<EmailState>) {
  const settingsUser = await requireAuthenticatedUser()

  if (!settingsUser) {
    return
  }

  emailErrorMessage.value = ''
  emailSuccessMessage.value = ''
  emailLoading.value = true

  const { error } = await supabase.auth.updateUser({
    email: event.data.email.trim(),
  })

  emailLoading.value = false

  if (error) {
    emailErrorMessage.value = error.message
    return
  }

  emailSuccessMessage.value =
    '確認メールを送信しました。メール内のリンクから変更を完了してください。'
}

async function updatePassword(event: FormSubmitEvent<PasswordState>) {
  const settingsUser = await requireAuthenticatedUser()

  if (!settingsUser) {
    return
  }

  passwordErrorMessage.value = ''
  passwordSuccessMessage.value = ''
  passwordLoading.value = true

  const { error } = await supabase.auth.updateUser({
    password: event.data.password,
  })

  passwordLoading.value = false

  if (error) {
    passwordErrorMessage.value = error.message
    return
  }

  passwordSuccessMessage.value = 'パスワードを変更しました。'
}
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto flex min-h-dvh w-full max-w-2xl flex-col justify-center px-5 py-10">
      <div class="space-y-8">
        <div>
          <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
          <h1 class="text-3xl font-semibold tracking-normal">アカウント設定</h1>
          <p class="mt-3 text-sm leading-6 text-neutral-300">
            メールアドレスとパスワードを管理します。
          </p>
        </div>

        <UAuthForm
          description="現在のメールアドレスに確認が必要な場合があります。"
          :fields="emailFields"
          icon="i-lucide-mail"
          :loading="emailLoading"
          :submit="{ label: 'メールアドレスを変更', color: 'primary', block: true }"
          title="メールアドレス変更"
          :validate="validateEmail"
          @submit="updateEmail"
        >
          <template #validation>
            <UAlert
              v-if="emailErrorMessage"
              color="error"
              icon="i-lucide-circle-alert"
              :description="emailErrorMessage"
            />
            <UAlert
              v-if="emailSuccessMessage"
              color="success"
              icon="i-lucide-circle-check"
              :description="emailSuccessMessage"
            />
          </template>
        </UAuthForm>

        <UAuthForm
          description="ログイン中のアカウントのパスワードを変更します。"
          :fields="passwordFields"
          icon="i-lucide-key-round"
          :loading="passwordLoading"
          :submit="{ label: 'パスワードを変更', color: 'primary', block: true }"
          title="パスワード変更"
          :validate="validatePassword"
          @submit="updatePassword"
        >
          <template #validation>
            <UAlert
              v-if="passwordErrorMessage"
              color="error"
              icon="i-lucide-circle-alert"
              :description="passwordErrorMessage"
            />
            <UAlert
              v-if="passwordSuccessMessage"
              color="success"
              icon="i-lucide-circle-check"
              :description="passwordSuccessMessage"
            />
          </template>

          <template #footer>
            <p class="text-center text-sm text-neutral-300">
              現在のパスワードがわからない場合は
              <NuxtLink
                class="font-medium text-primary-300 hover:text-primary-200"
                to="/reset-password"
              >
                再設定
              </NuxtLink>
            </p>
          </template>
        </UAuthForm>

        <UButton color="neutral" icon="i-lucide-house" size="xl" to="/" variant="subtle">
          トップへ戻る
        </UButton>
      </div>
    </section>
  </main>
</template>
