// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },
  css: ['~/app.css'],
  modules: ['@nuxt/eslint', '@nuxt/ui', '@nuxtjs/supabase'],
  srcDir: 'bmz-ir-web/app',
  serverDir: 'bmz-ir-web/server',
  dir: {
    public: 'bmz-ir-web/public',
    shared: 'bmz-ir-web/shared',
  },
  supabase: {
    redirect: false,
    types: '~~/bmz-ir-web/shared/types/database.types.ts',
  },
  vite: {
    optimizeDeps: {
      include: ['@vue/devtools-core', '@vue/devtools-kit'],
    },
  },
})
