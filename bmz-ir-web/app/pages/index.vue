<script setup lang="ts">
const user = useSupabaseUser()
</script>

<template>
  <main class="min-h-dvh bg-neutral-950 text-neutral-50">
    <section class="mx-auto flex min-h-dvh w-full max-w-2xl flex-col justify-center px-5 py-10">
      <div class="space-y-8">
        <div>
          <p class="mb-2 text-sm font-medium text-primary-300">BMZ Internet Ranking</p>
          <h1 class="text-4xl font-semibold tracking-normal">BMZ IR</h1>
          <p class="mt-3 max-w-xl text-sm leading-6 text-neutral-300">
            BMZ Player のスコア送信とランキング確認に使うアカウントを管理します。
          </p>
        </div>

        <div v-if="user" class="space-y-4">
          <UAlert
            color="success"
            icon="i-lucide-circle-check"
            :description="`${user.email ?? 'ログイン中のユーザー'} としてログインしています。`"
          />
          <div class="flex flex-col gap-3 sm:flex-row">
            <UButton color="primary" icon="i-lucide-user-pen" size="xl" to="/profile">
              プロフィール編集
            </UButton>
            <UButton
              color="neutral"
              icon="i-lucide-settings"
              size="xl"
              to="/settings"
              variant="subtle"
            >
              アカウント設定
            </UButton>
            <UButton
              color="neutral"
              icon="i-lucide-log-out"
              size="xl"
              to="/signout"
              variant="subtle"
            >
              ログアウト
            </UButton>
          </div>
        </div>

        <div v-else class="flex flex-col gap-3 sm:flex-row">
          <UButton color="primary" icon="i-lucide-log-in" size="xl" to="/signin">
            ログイン
          </UButton>
          <UButton
            color="neutral"
            icon="i-lucide-user-plus"
            size="xl"
            to="/signup"
            variant="subtle"
          >
            登録
          </UButton>
        </div>

        <div class="flex flex-col gap-3 sm:flex-row">
          <UButton color="neutral" icon="i-lucide-list-music" size="xl" to="/charts" variant="subtle">
            譜面一覧・ランキング
          </UButton>
          <UButton
            v-if="user"
            color="neutral"
            icon="i-lucide-trophy"
            size="xl"
            :to="`/players/${user.sub ?? user.id}`"
            variant="subtle"
          >
            自分のスコア
          </UButton>
        </div>
      </div>
    </section>
  </main>
</template>
