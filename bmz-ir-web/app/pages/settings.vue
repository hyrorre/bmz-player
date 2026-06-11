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

type DeviceKey = {
  id: string
  public_key: string
  algorithm: string
  revoked_at: string | null
  created_at: string
}

const deviceKeys = ref<DeviceKey[]>([])
const deviceKeysLoading = ref(false)
const deviceKeysError = ref('')
const revokingKeyId = ref('')
const requestFetch = useRequestFetch()

await loadDeviceKeys()

async function loadDeviceKeys() {
  deviceKeysLoading.value = true
  deviceKeysError.value = ''
  try {
    const response = await requestFetch<{ device_keys: DeviceKey[] }>('/api/v1/device-keys')
    deviceKeys.value = response.device_keys
  } catch (error) {
    deviceKeysError.value = error instanceof Error ? error.message : '署名鍵の取得に失敗しました。'
  } finally {
    deviceKeysLoading.value = false
  }
}

async function revokeDeviceKey(keyId: string) {
  revokingKeyId.value = keyId
  deviceKeysError.value = ''
  try {
    await requestFetch(`/api/v1/device-keys/${keyId}`, { method: 'DELETE' })
    await loadDeviceKeys()
  } catch (error) {
    deviceKeysError.value = error instanceof Error ? error.message : '署名鍵の失効に失敗しました。'
  } finally {
    revokingKeyId.value = ''
  }
}

function keyFingerprint(publicKey: string) {
  return `${publicKey.slice(0, 8)}…${publicKey.slice(-8)}`
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

        <section class="space-y-3">
          <div>
            <h2 class="text-xl font-semibold">スコア署名鍵 (device key)</h2>
            <p class="mt-1 text-sm leading-6 text-neutral-300">
              BMZ Player がスコア送信の改ざん防止署名に使う鍵です。
              端末を紛失した場合などはここから失効できます。失効後はその端末で
              <code class="rounded bg-neutral-900 px-1">bmz ir device-key rotate</code>
              を実行すると新しい鍵が登録されます。
            </p>
          </div>
          <UAlert v-if="deviceKeysError" color="error" :description="deviceKeysError" />
          <p v-if="deviceKeysLoading" class="text-sm text-neutral-400">読み込み中...</p>
          <p v-else-if="!deviceKeys.length" class="text-sm text-neutral-400">
            登録済みの署名鍵はありません。
          </p>
          <ul v-else class="divide-y divide-neutral-800 rounded-lg border border-neutral-800">
            <li
              v-for="key in deviceKeys"
              :key="key.id"
              class="flex items-center justify-between gap-4 px-4 py-3"
            >
              <div class="min-w-0">
                <p class="font-mono text-sm">{{ keyFingerprint(key.public_key) }}</p>
                <p class="text-xs text-neutral-500">
                  {{ key.algorithm }} ・ 登録 {{ new Date(key.created_at).toLocaleString() }}
                  <UBadge v-if="key.revoked_at" color="error" size="sm" variant="subtle">
                    失効済み
                  </UBadge>
                </p>
              </div>
              <UButton
                v-if="!key.revoked_at"
                color="error"
                variant="subtle"
                size="sm"
                :loading="revokingKeyId === key.id"
                @click="revokeDeviceKey(key.id)"
              >
                失効
              </UButton>
            </li>
          </ul>
        </section>

        <UButton color="neutral" icon="i-lucide-house" size="xl" to="/" variant="subtle">
          トップへ戻る
        </UButton>
      </div>
    </section>
  </main>
</template>
