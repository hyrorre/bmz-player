<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'

type LoginState = {
  email: string
  password: string
}

const { loggedIn, fetch: refreshSession } = useUserSession()
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()

const fields = computed<AuthFormField[]>(() => [
  {
    name: 'email',
    type: 'email',
    label: t('auth.email'),
    placeholder: 'name@example.com',
    autocomplete: 'email',
    required: true,
    defaultValue: '',
  },
  {
    name: 'password',
    type: 'password',
    label: t('auth.password'),
    placeholder: t('auth.password'),
    autocomplete: 'current-password',
    required: true,
    defaultValue: '',
  },
])

const loading = ref(false)
const errorMessage = ref('')

if (loggedIn.value) {
  await navigateTo(localePath('/'))
}

function validate(state: Partial<LoginState>) {
  const errors: { name: keyof LoginState; message: string }[] = []

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: t('validation.emailRequired') })
  }

  if (!state.password) {
    errors.push({ name: 'password', message: t('validation.passwordRequired') })
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
    errorMessage.value = translateApiError(error, 'errors.loginFailed')
    loading.value = false
    return
  }

  loading.value = false
  await navigateTo(localePath('/'))
}

useSeoMeta({ title: () => t('auth.login') })
</script>

<template>
  <main>
    <section class="mx-auto flex w-full max-w-md flex-col justify-center px-5 py-10">
      <UAuthForm
        class="w-full"
        :description="t('login.description')"
        :fields="fields"
        icon="i-lucide-log-in"
        :loading="loading"
        :submit="{ label: t('auth.login'), color: 'primary', block: true }"
        :title="t('auth.login')"
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
              {{ t('login.noAccount') }}
              <NuxtLink
                class="font-medium text-primary-300 hover:text-primary-200"
                :to="localePath('/register')"
              >
                {{ t('nav.register') }}
              </NuxtLink>
            </p>
            <p>
              {{ t('login.forgotPassword') }}
              <NuxtLink
                class="font-medium text-primary-300 hover:text-primary-200"
                :to="localePath('/reset-password')"
              >
                {{ t('login.reset') }}
              </NuxtLink>
            </p>
          </div>
        </template>
      </UAuthForm>
    </section>
  </main>
</template>
