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
      casing: 'snake_case',
      driver: 'd1',
      migrationsDirs: ['server/db/migrations'],
      connection: {
        databaseId:
          process.env.NUXT_HUB_CLOUDFLARE_DATABASE_ID || '1b7a8d66-98a4-4641-82ee-32eebe0b89e2',
      },
    },
    blob: {
      driver: 'cloudflare-r2',
      binding: 'BLOB',
      bucketName: process.env.NUXT_HUB_BLOB_BUCKET || 'bmz-ir-blob',
    },
  },
  runtimeConfig: {
    session: {
      name: 'bmz-session',
      password: process.env.NUXT_SESSION_PASSWORD || '',
      maxAge: 60 * 60 * 24 * 180, // 180 days
    },
  },
  nitro: {
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
