<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'

type EmailState = {
  currentPassword: string
  email: string
}

type PasswordState = {
  currentPassword: string
  password: string
}

type DeleteAccountState = {
  currentPassword: string
}

const { user, fetch: refreshSession, clear } = useUserSession()
const requestFetch = useRequestFetch()
const localePath = useLocalePath()
const { t } = useI18n()
const { formatDateTime } = useLocaleFormat()
const { translateApiError } = useApiError()

const emailFields = computed<AuthFormField[]>(() => [
  {
    name: 'currentPassword',
    type: 'password',
    label: t('auth.currentPassword'),
    placeholder: t('auth.currentPassword'),
    autocomplete: 'current-password',
    required: true,
    defaultValue: '',
  },
  {
    name: 'email',
    type: 'email',
    label: t('settings.newEmail'),
    placeholder: 'name@example.com',
    autocomplete: 'email',
    required: true,
    defaultValue: (user.value as { email?: string } | null)?.email ?? '',
  },
])

const passwordFields = computed<AuthFormField[]>(() => [
  {
    name: 'currentPassword',
    type: 'password',
    label: t('auth.currentPassword'),
    placeholder: t('auth.currentPassword'),
    autocomplete: 'current-password',
    required: true,
    defaultValue: '',
  },
  {
    name: 'password',
    type: 'password',
    label: t('auth.newPassword'),
    placeholder: t('auth.passwordMin'),
    autocomplete: 'new-password',
    required: true,
    defaultValue: '',
  },
])

const deleteAccountFields = computed<AuthFormField[]>(() => [
  {
    name: 'currentPassword',
    type: 'password',
    label: t('auth.currentPassword'),
    placeholder: t('auth.currentPassword'),
    autocomplete: 'current-password',
    required: true,
    defaultValue: '',
  },
])

const emailLoading = ref(false)
const passwordLoading = ref(false)
const deleteAccountLoading = ref(false)
const emailErrorMessage = ref('')
const emailSuccessMessage = ref('')
const passwordErrorMessage = ref('')
const passwordSuccessMessage = ref('')
const deleteAccountErrorMessage = ref('')

if (!user.value) {
  await navigateTo(localePath('/login'))
}

function validateEmail(state: Partial<EmailState>) {
  const errors: { name: keyof EmailState; message: string }[] = []

  if (!state.currentPassword) {
    errors.push({ name: 'currentPassword', message: t('validation.currentPasswordRequired') })
  }

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: t('validation.emailRequired') })
  }

  return errors
}

function validatePassword(state: Partial<PasswordState>) {
  const errors: { name: keyof PasswordState; message: string }[] = []

  if (!state.currentPassword) {
    errors.push({ name: 'currentPassword', message: t('validation.currentPasswordRequired') })
  }

  if (!state.password || state.password.length < 8) {
    errors.push({ name: 'password', message: t('validation.passwordMin') })
  }

  return errors
}

function validateDeleteAccount(state: Partial<DeleteAccountState>) {
  const errors: { name: keyof DeleteAccountState; message: string }[] = []

  if (!state.currentPassword) {
    errors.push({ name: 'currentPassword', message: t('validation.currentPasswordRequired') })
  }

  return errors
}

async function updateEmail(event: FormSubmitEvent<EmailState>) {
  if (!user.value) {
    await navigateTo(localePath('/login'))
    return
  }

  emailErrorMessage.value = ''
  emailSuccessMessage.value = ''
  emailLoading.value = true

  try {
    await requestFetch('/api/v1/account/email', {
      method: 'PUT',
      body: {
        current_password: event.data.currentPassword,
        email: event.data.email.trim(),
      },
    })
    await refreshSession()
    emailSuccessMessage.value = t('settings.emailChanged')
  } catch (error) {
    emailErrorMessage.value = translateApiError(error, 'errors.emailChangeFailed')
  } finally {
    emailLoading.value = false
  }
}

async function updatePassword(event: FormSubmitEvent<PasswordState>) {
  if (!user.value) {
    await navigateTo(localePath('/login'))
    return
  }

  passwordErrorMessage.value = ''
  passwordSuccessMessage.value = ''
  passwordLoading.value = true

  try {
    await requestFetch('/api/v1/account/password', {
      method: 'PUT',
      body: {
        current_password: event.data.currentPassword,
        password: event.data.password,
      },
    })
    await clear()
    await navigateTo(localePath('/login'))
  } catch (error) {
    passwordErrorMessage.value = translateApiError(error, 'errors.passwordChangeFailed')
  } finally {
    passwordLoading.value = false
  }
}

async function deleteAccount(event: FormSubmitEvent<DeleteAccountState>) {
  if (!user.value) {
    await navigateTo(localePath('/login'))
    return
  }

  deleteAccountErrorMessage.value = ''
  deleteAccountLoading.value = true

  try {
    await requestFetch('/api/v1/account', {
      method: 'DELETE',
      body: {
        current_password: event.data.currentPassword,
      },
    })
    await clear()
    await navigateTo(localePath('/'))
  } catch (error) {
    deleteAccountErrorMessage.value = translateApiError(error, 'errors.accountDeleteFailed')
  } finally {
    deleteAccountLoading.value = false
  }
}

type DeviceKey = {
  id: string
  public_key: string
  algorithm: string
  revoked_at: string | null
  created_at: string
}

type SessionSummary = {
  id: string
  clientType: 'web' | 'desktop'
  createdAt: string
  expiresAt: string
  lastUsedAt: string | null
  hasAccessToken: boolean
  hasRefreshToken: boolean
}

const sessions = ref<SessionSummary[]>([])
const sessionsLoading = ref(false)
const sessionsError = ref('')
const revokingSessionId = ref('')
const deviceKeys = ref<DeviceKey[]>([])
const deviceKeysLoading = ref(false)
const deviceKeysError = ref('')
const revokingKeyId = ref('')
await loadSessions()
await loadDeviceKeys()

async function loadSessions() {
  sessionsLoading.value = true
  sessionsError.value = ''
  try {
    const response = await requestFetch<{ sessions: SessionSummary[] }>('/api/v1/sessions')
    sessions.value = response.sessions
  } catch (error) {
    sessionsError.value = translateApiError(error, 'errors.sessionsLoadFailed')
  } finally {
    sessionsLoading.value = false
  }
}

async function revokeSession(sessionId: string) {
  revokingSessionId.value = sessionId
  sessionsError.value = ''
  try {
    await requestFetch(`/api/v1/sessions/${sessionId}`, { method: 'DELETE' })
    await loadSessions()
  } catch (error) {
    sessionsError.value = translateApiError(error, 'errors.sessionRevokeFailed')
  } finally {
    revokingSessionId.value = ''
  }
}

async function loadDeviceKeys() {
  deviceKeysLoading.value = true
  deviceKeysError.value = ''
  try {
    const response = await requestFetch<{ device_keys: DeviceKey[] }>('/api/v1/device-keys')
    deviceKeys.value = response.device_keys
  } catch (error) {
    deviceKeysError.value = translateApiError(error, 'errors.deviceKeysLoadFailed')
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
    deviceKeysError.value = translateApiError(error, 'errors.deviceKeyRevokeFailed')
  } finally {
    revokingKeyId.value = ''
  }
}

function keyFingerprint(publicKey: string) {
  return `${publicKey.slice(0, 8)}…${publicKey.slice(-8)}`
}

function sessionLabel(session: SessionSummary) {
  return session.clientType === 'desktop' ? 'BMZ Player' : 'Web'
}

function sessionTokenLabel(session: SessionSummary) {
  if (session.hasAccessToken && session.hasRefreshToken) {
    return 'access + refresh'
  }
  if (session.hasRefreshToken) {
    return 'refresh'
  }
  return 'access'
}

useSeoMeta({ title: () => t('settings.title') })
</script>

<template>
  <main>
    <section class="mx-auto flex w-full max-w-2xl flex-col justify-center px-5 py-10">
      <div class="space-y-8">
        <div>
          <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
          <h1 class="text-3xl font-semibold tracking-normal">{{ t('settings.title') }}</h1>
          <p class="mt-3 text-sm leading-6 text-neutral-300">
            {{ t('settings.description') }}
          </p>
        </div>

        <UCard>
          <UAuthForm
            class="w-full"
            :description="t('settings.currentPasswordNeeded')"
            :fields="emailFields"
            icon="i-lucide-mail"
            :loading="emailLoading"
            :submit="{ label: t('settings.changeEmail'), color: 'primary', block: true }"
            :title="t('settings.emailChange')"
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
        </UCard>

        <UCard>
          <UAuthForm
            class="w-full"
            :description="t('settings.passwordChangeDescription')"
            :fields="passwordFields"
            icon="i-lucide-key-round"
            :loading="passwordLoading"
            :submit="{ label: t('settings.changePassword'), color: 'primary', block: true }"
            :title="t('reset.changePassword')"
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
                {{ t('settings.forgotCurrentPassword') }}
                <NuxtLink
                  class="font-medium text-primary-300 hover:text-primary-200"
                  :to="localePath('/reset-password')"
                >
                  {{ t('login.reset') }}
                </NuxtLink>
              </p>
            </template>
          </UAuthForm>
        </UCard>

        <UCard>
          <section class="space-y-3">
            <div>
              <h2 class="text-xl font-semibold">{{ t('settings.sessions') }}</h2>
              <p class="mt-1 text-sm leading-6 text-neutral-300">
                {{ t('settings.sessionsDescription') }}
              </p>
            </div>
            <UAlert v-if="sessionsError" color="error" :description="sessionsError" />
            <p v-if="sessionsLoading" class="text-sm text-neutral-400">{{ t('common.loading') }}</p>
            <p v-else-if="!sessions.length" class="text-sm text-neutral-400">
              {{ t('settings.noSessions') }}
            </p>
            <ul v-else class="divide-y divide-neutral-800 rounded-lg border border-neutral-800">
              <li
                v-for="session in sessions"
                :key="session.id"
                class="flex items-center justify-between gap-4 px-4 py-3"
              >
                <div class="min-w-0">
                  <p class="text-sm font-medium">{{ sessionLabel(session) }}</p>
                  <p class="text-xs text-neutral-500">
                    {{ sessionTokenLabel(session) }} ・ {{ t('settings.created') }}
                    {{ formatDateTime(session.createdAt) }} ・ {{ t('settings.lastUsed') }}
                    {{
                      session.lastUsedAt
                        ? formatDateTime(session.lastUsedAt)
                        : t('common.notRecorded')
                    }}
                    ・ {{ t('settings.expires') }} {{ formatDateTime(session.expiresAt) }}
                  </p>
                </div>
                <UButton
                  color="error"
                  variant="subtle"
                  size="sm"
                  :loading="revokingSessionId === session.id"
                  @click="revokeSession(session.id)"
                >
                  {{ t('common.revoke') }}
                </UButton>
              </li>
            </ul>
          </section>
        </UCard>

        <UCard>
          <section class="space-y-3">
            <div>
              <h2 class="text-xl font-semibold">{{ t('settings.deviceKeys') }}</h2>
              <p class="mt-1 text-sm leading-6 text-neutral-300">
                {{ t('settings.deviceKeysDescriptionBefore') }}
                <code class="rounded bg-neutral-900 px-1">bmz ir device-key rotate</code>
                {{ t('settings.deviceKeysDescriptionAfter') }}
              </p>
            </div>
            <UAlert v-if="deviceKeysError" color="error" :description="deviceKeysError" />
            <p v-if="deviceKeysLoading" class="text-sm text-neutral-400">
              {{ t('common.loading') }}
            </p>
            <p v-else-if="!deviceKeys.length" class="text-sm text-neutral-400">
              {{ t('settings.noDeviceKeys') }}
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
                    {{ key.algorithm }} ・ {{ t('settings.registered') }}
                    {{ formatDateTime(key.created_at) }}
                    <UBadge v-if="key.revoked_at" color="error" size="sm" variant="subtle">
                      {{ t('settings.revoked') }}
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
                  {{ t('common.revoke') }}
                </UButton>
              </li>
            </ul>
          </section>
        </UCard>

        <UCard>
          <UAuthForm
            class="w-full"
            :description="t('settings.deleteDescription')"
            :fields="deleteAccountFields"
            icon="i-lucide-trash-2"
            :loading="deleteAccountLoading"
            :submit="{ label: t('settings.deleteAccount'), color: 'error', block: true }"
            :title="t('settings.accountDeletion')"
            :validate="validateDeleteAccount"
            @submit="deleteAccount"
          >
            <template #validation>
              <UAlert
                v-if="deleteAccountErrorMessage"
                color="error"
                icon="i-lucide-circle-alert"
                :description="deleteAccountErrorMessage"
              />
            </template>
          </UAuthForm>
        </UCard>
      </div>
    </section>
  </main>
</template>
