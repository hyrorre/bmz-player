// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },
  css: ['~/app.css'],
  modules: ['@nuxt/eslint', '@nuxt/ui', '@nuxtjs/i18n', '@nuxthub/core', 'nuxt-auth-utils'],
  srcDir: 'bmz-ir-web/app',
  serverDir: 'bmz-ir-web/server',
  dir: {
    public: 'bmz-ir-web/public',
    shared: 'bmz-ir-web/shared',
  },
  i18n: {
    baseUrl: process.env.NUXT_PUBLIC_SITE_URL || '',
    strategy: 'prefix_except_default',
    defaultLocale: 'ja',
    locales: [
      { code: 'ja', language: 'ja-JP', name: '日本語', file: 'ja.ts' },
      { code: 'en', language: 'en', name: 'English', file: 'en.ts' },
      { code: 'ko', language: 'ko-KR', name: '한국어', file: 'ko.ts' },
      { code: 'zh-CN', language: 'zh-CN', name: '简体中文', file: 'zh-CN.ts' },
      { code: 'zh-TW', language: 'zh-TW', name: '繁體中文（台灣）', file: 'zh-TW.ts' },
      { code: 'zh-HK', language: 'zh-HK', name: '繁體中文（香港）', file: 'zh-HK.ts' },
    ],
    detectBrowserLanguage: {
      useCookie: true,
      cookieKey: 'bmz_ir_locale',
      redirectOn: 'root',
      fallbackLocale: 'ja',
    },
  },
  hub: {
    db: {
      dialect: 'sqlite',
      driver: 'd1',
      casing: 'snake_case',
    },
    blob: true,
  },
  runtimeConfig: {
    ir: {
      providerKey: process.env.NUXT_IR_PROVIDER_KEY || '',
      adminUserIds: process.env.NUXT_IR_ADMIN_USER_IDS || '',
    },
    session: {
      name: 'bmz-session',
      password: process.env.NUXT_SESSION_PASSWORD || '',
      maxAge: 60 * 60 * 24 * 180, // 180 days
    },
  },
  nitro: {
    experimental: {
      tasks: true,
    },
    scheduledTasks: {
      '17 */6 * * *': 'difficulty-tables:sync',
    },
    compatibilityDate: '2026-03-29',
    preset: 'cloudflare_module',
    cloudflare: {
      deployConfig: true,
      nodeCompat: true,
    },
  },
  vite: {
    optimizeDeps: {
      include: ['@vue/devtools-core', '@vue/devtools-kit'],
    },
  },
})
