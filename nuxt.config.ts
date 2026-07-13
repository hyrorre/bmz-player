// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },
  css: ['~/app.css'],
  modules: ['@nuxt/eslint', '@nuxt/ui', '@nuxthub/core', 'nuxt-auth-utils'],
  srcDir: 'bmz-ir-web/app',
  serverDir: 'bmz-ir-web/server',
  dir: {
    public: 'bmz-ir-web/public',
    shared: 'bmz-ir-web/shared',
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
