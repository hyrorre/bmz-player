<script setup lang="ts">
const route = useRoute()
const localePath = useLocalePath()
const { t } = useI18n()
const { manualLocale, pages, showsFallback, pagePath } = useManual()

const slug = computed(() => String(route.params.slug))
const index = computed(() => pages.value.findIndex((page) => page.slug === slug.value))
const page = computed(() => pages.value[index.value])
const previous = computed(() => (index.value > 0 ? pages.value[index.value - 1] : undefined))
const next = computed(() => pages.value[index.value + 1])
const chapterItems = computed(() =>
  pages.value.map(({ title, slug: value }) => ({ label: title, value })),
)

if (!page.value) {
  throw createError({ statusCode: 404, statusMessage: 'Not Found' })
}

const html = computed(() =>
  page.value ? renderManualMarkdown(page.value.markdown, manualLocale.value, pagePath) : '',
)

// 原稿内のページ間リンクは、ページを読み直さずに移動する。
function onContentClick(event: MouseEvent) {
  if (event.defaultPrevented || event.button !== 0) return
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
  const link = (event.target as Element | null)?.closest('a[data-manual-link]')
  const href = link?.getAttribute('href')
  if (!href) return
  event.preventDefault()
  navigateTo(href)
}

useSeoMeta({
  title: () => (page.value ? `${page.value.title} | ${t('manual.title')}` : t('manual.title')),
  description: () => page.value?.summary ?? '',
})
</script>

<template>
  <main>
    <section v-if="page" class="mx-auto w-full max-w-4xl px-5 py-10">
      <nav
        :aria-label="t('manual.title')"
        class="mb-6 flex flex-wrap items-center justify-between gap-3"
      >
        <ULink
          :to="localePath('/manual')"
          class="inline-flex items-center gap-1 text-sm font-medium text-primary hover:opacity-80"
        >
          <UIcon name="i-lucide-book-open" />
          {{ t('manual.title') }}
        </ULink>
        <USelect
          :aria-label="t('manual.chapters')"
          :items="chapterItems"
          :model-value="slug"
          class="w-full sm:w-64"
          size="sm"
          @update:model-value="(chapter) => navigateTo(pagePath(chapter))"
        />
      </nav>
      <UAlert
        v-if="showsFallback"
        class="mb-6"
        color="neutral"
        icon="i-lucide-languages"
        :title="t('manual.englishOnly')"
        variant="subtle"
      />
      <h1 class="mb-6 text-3xl font-semibold">{{ page.title }}</h1>
      <!-- eslint-disable-next-line vue/no-v-html -- リポジトリ内の原稿だけを表示する -->
      <div class="manual-content" @click="onContentClick" v-html="html" />

      <div class="mt-12 flex flex-col gap-3 border-t border-default pt-6 sm:flex-row">
        <UButton
          v-if="previous"
          color="neutral"
          icon="i-lucide-arrow-left"
          :to="pagePath(previous.slug)"
          variant="subtle"
        >
          {{ t('manual.previous', { title: previous.title }) }}
        </UButton>
        <UButton
          v-if="next"
          class="sm:ml-auto"
          color="neutral"
          trailing-icon="i-lucide-arrow-right"
          :to="pagePath(next.slug)"
          variant="subtle"
        >
          {{ t('manual.next', { title: next.title }) }}
        </UButton>
      </div>
    </section>
  </main>
</template>
