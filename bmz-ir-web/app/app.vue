<script setup lang="ts">
import { en, ja, ko, zh_cn, zh_tw } from '@nuxt/ui/locale'

const { locale, t } = useI18n()
const uiLocales = {
  ja,
  en,
  ko,
  'zh-CN': zh_cn,
  'zh-TW': zh_tw,
  'zh-HK': extendLocale(zh_tw, { name: '繁體中文（香港）', code: 'zh-HK' }),
}
const uiLocale = computed(() => uiLocales[locale.value as keyof typeof uiLocales] ?? ja)
const i18nHead = useLocaleHead({ seo: true })

useHead(() => ({
  htmlAttrs: { ...i18nHead.value.htmlAttrs, dir: uiLocale.value.dir },
  link: i18nHead.value.link ?? [],
  meta: i18nHead.value.meta ?? [],
  titleTemplate: (title) => (title ? `${title} - BMZ IR` : 'BMZ IR'),
}))
useSeoMeta({ description: () => t('meta.description') })
</script>

<template>
  <UApp :locale="uiLocale">
    <NuxtRouteAnnouncer />
    <AppHeader />
    <UContainer class="flex">
      <AppSidebar />
      <NuxtPage class="grow" />
    </UContainer>
    <AppFooter />
  </UApp>
</template>
