// https://nuxt.com/docs/api/configuration/nuxt-config
const isCloudflareBuild =
  process.env.NITRO_PRESET?.includes('cloudflare') ||
  process.env.CF_PAGES === '1' ||
  process.env.CLOUDFLARE_ENV !== undefined
const cloudflareD1DatabaseName = process.env.NUXT_HUB_CLOUDFLARE_DATABASE_NAME || 'bmz-ir'
const cloudflareD1DatabaseId =
  process.env.NUXT_HUB_CLOUDFLARE_DATABASE_ID || '00000000-0000-0000-0000-000000000000'
const cloudflareBlobBucketName = process.env.NUXT_HUB_BLOB_BUCKET || 'bmz-ir-replays'

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
    db: isCloudflareBuild
      ? {
          dialect: 'sqlite',
          driver: 'd1',
          connection: {
            databaseId: cloudflareD1DatabaseId,
          },
        }
      : 'sqlite',
    blob: isCloudflareBuild
      ? {
          driver: 'cloudflare-r2',
          binding: 'BLOB',
          bucketName: cloudflareBlobBucketName,
        }
      : {
          driver: 'fs',
          dir: '.data/blob',
        },
  },
  nitro: {
    cloudflare: {
      wrangler: {
        name: process.env.CLOUDFLARE_WORKER_NAME || 'bmz-ir-web',
        compatibility_date: '2026-06-13',
        compatibility_flags: ['nodejs_compat'],
        d1_databases: [
          {
            binding: 'DB',
            database_name: cloudflareD1DatabaseName,
            database_id: cloudflareD1DatabaseId,
          },
        ],
        r2_buckets: [
          {
            binding: 'BLOB',
            bucket_name: cloudflareBlobBucketName,
          },
        ],
      },
    },
  },
  vite: {
    optimizeDeps: {
      include: ['@vue/devtools-core', '@vue/devtools-kit'],
    },
  },
})
