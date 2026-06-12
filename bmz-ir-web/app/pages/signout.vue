<script setup lang="ts">
type SessionUser = {
  email?: string
}

const { user, clear } = useUserSession()

const loading = ref(false)
const errorMessage = ref('')

async function signOut() {
  errorMessage.value = ''
  loading.value = true

  try {
    await clear()
  } catch (error) {
    errorMessage.value =
      error instanceof Error && error.message ? error.message : 'ログアウトに失敗しました。'
    loading.value = false
    return
  }

  loading.value = false
  await navigateTo('/signin')
}
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto flex min-h-dvh w-full max-w-md flex-col justify-center px-5 py-10">
      <div class="space-y-6">
        <div>
          <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
          <h1 class="text-3xl font-semibold tracking-normal">ログアウト</h1>
          <p class="mt-3 text-sm leading-6 text-neutral-300">
            {{
              user
                ? `${(user as SessionUser).email ?? 'ログイン中のユーザー'} からログアウトします。`
                : '現在ログインしていません。'
            }}
          </p>
        </div>

        <UAlert
          v-if="errorMessage"
          color="error"
          icon="i-lucide-circle-alert"
          :description="errorMessage"
        />

        <div class="flex flex-col gap-3 sm:flex-row">
          <UButton
            color="primary"
            :disabled="!user"
            icon="i-lucide-log-out"
            :loading="loading"
            size="xl"
            type="button"
            @click="signOut"
          >
            ログアウト
          </UButton>
          <UButton color="neutral" icon="i-lucide-house" size="xl" to="/" variant="subtle">
            トップへ戻る
          </UButton>
        </div>
      </div>
    </section>
  </main>
</template>
