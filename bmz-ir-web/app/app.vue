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
  link: [
    ...(i18nHead.value.link ?? []),
    { rel: 'icon', href: '/favicon.ico', sizes: '16x16 32x32 48x48' },
    { rel: 'icon', href: '/icon.svg', type: 'image/svg+xml' },
    { rel: 'apple-touch-icon', href: '/apple-touch-icon.png', sizes: '180x180' },
  ],
  meta: [...(i18nHead.value.meta ?? []), { name: 'theme-color', content: '#ffffff' }],
  titleTemplate: (title) => (title ? `${title} - BMZ IR` : 'BMZ IR'),
}))
useSeoMeta({ description: () => t('meta.description') })
</script>

<template>
  <UApp :locale="uiLocale">
    <NuxtRouteAnnouncer />
    <AppHeader />
    <NuxtPage />
    <AppFooter />
  </UApp>
</template>
