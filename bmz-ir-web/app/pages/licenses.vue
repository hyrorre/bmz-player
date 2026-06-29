<script setup lang="ts">
const reportUrl = '/licenses/web-dependency-licenses.txt'
const licenseText = ref('')
const loading = ref(true)
const errorMessage = ref('')

const packageCount = computed(() => {
  const match = /^Packages: (?<count>\d+)$/mu.exec(licenseText.value)
  return match?.groups?.count ?? ''
})

useSeoMeta({
  title: 'Web Dependency Licenses - BMZ IR',
})

onMounted(async () => {
  loading.value = true
  errorMessage.value = ''

  try {
    const response = await fetch(reportUrl, {
      headers: { accept: 'text/plain' },
    })
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`)
    }
    licenseText.value = await response.text()
  } catch (error) {
    errorMessage.value =
      error instanceof Error
        ? `web-dependency-licenses.txt を読み込めませんでした (${error.message})。`
        : 'web-dependency-licenses.txt を読み込めませんでした。'
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <main class="w-full px-5 py-8">
    <section class="mx-auto flex w-full max-w-5xl flex-col gap-5">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p class="text-sm font-medium text-primary-300">BMZ IR</p>
          <h1 class="mt-1 text-3xl font-semibold tracking-normal">Web Dependency Licenses</h1>
          <p class="mt-2 max-w-2xl text-sm leading-6 text-neutral-300">
            Cloudflare Worker bundle に含まれる Web 依存パッケージの third-party notice です。
          </p>
        </div>
        <UButton
          color="neutral"
          external
          icon="i-lucide-file-text"
          :to="reportUrl"
          target="_blank"
          variant="subtle"
        >
          txt を開く
        </UButton>
      </div>

      <UAlert
        v-if="errorMessage"
        color="warning"
        icon="i-lucide-triangle-alert"
        :description="errorMessage"
        variant="subtle"
      />

      <div
        v-else
        class="overflow-hidden rounded-lg border border-neutral-800 bg-neutral-950/70 shadow-sm"
      >
        <div class="flex items-center justify-between border-b border-neutral-800 px-4 py-3">
          <p class="text-sm font-medium text-neutral-200">web-dependency-licenses.txt</p>
          <p v-if="packageCount" class="text-xs text-neutral-400">{{ packageCount }} packages</p>
        </div>
        <div v-if="loading" class="space-y-3 p-4">
          <USkeleton class="h-4 w-48" />
          <USkeleton class="h-4 w-full" />
          <USkeleton class="h-4 w-4/5" />
          <USkeleton class="h-4 w-2/3" />
        </div>
        <pre
          v-else
          class="max-h-[70vh] overflow-auto whitespace-pre-wrap p-4 font-mono text-xs leading-5 text-neutral-100"
          >{{ licenseText }}</pre
        >
      </div>
    </section>
  </main>
</template>
