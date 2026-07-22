<script setup lang="ts">
import type { AuthFormField, FormSubmitEvent } from '@nuxt/ui'

type RegisterState = {
  displayName: string
  email: string
  password: string
}

const { loggedIn, fetch: refreshSession } = useUserSession()
const localePath = useLocalePath()
const { t } = useI18n()
const { translateApiError } = useApiError()

const fields = computed<AuthFormField[]>(() => [
  {
    name: 'displayName',
    type: 'text',
    label: t('auth.displayName'),
    placeholder: 'name',
    autocomplete: 'nickname',
    required: true,
    defaultValue: '',
  },
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
    placeholder: t('auth.passwordMin'),
    autocomplete: 'new-password',
    required: true,
    defaultValue: '',
  },
])

const loading = ref(false)
const errorMessage = ref('')
const successMessage = ref('')

if (loggedIn.value) {
  await navigateTo(localePath('/'))
}

function validate(state: Partial<RegisterState>) {
  const errors: { name: keyof RegisterState; message: string }[] = []

  if (!state.displayName?.trim()) {
    errors.push({ name: 'displayName', message: t('validation.displayNameRequired') })
  }

  if (!state.email?.trim()) {
    errors.push({ name: 'email', message: t('validation.emailRequired') })
  }

  if (!state.password || state.password.length < 8) {
    errors.push({ name: 'password', message: t('validation.passwordMin') })
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
    errorMessage.value = translateApiError(error, 'errors.registerFailed')
    loading.value = false
    return
  }

  loading.value = false
  successMessage.value = ''
  await navigateTo(localePath('/'))
}

useSeoMeta({ title: () => t('register.title') })
</script>

<template>
  <main>
    <section class="mx-auto flex w-full max-w-md flex-col justify-center px-5 py-10">
      <UAuthForm
        class="w-full"
        :description="t('register.description')"
        :fields="fields"
        icon="i-lucide-user-plus"
        :loading="loading"
        :submit="{ label: t('register.submit'), color: 'primary', block: true }"
        :title="t('register.title')"
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
            {{ t('register.haveAccount') }}
            <NuxtLink
              class="font-medium text-primary-300 hover:text-primary-200"
              :to="localePath('/login')"
            >
              {{ t('nav.login') }}
            </NuxtLink>
          </p>
        </template>
      </UAuthForm>
    </section>
  </main>
</template>
