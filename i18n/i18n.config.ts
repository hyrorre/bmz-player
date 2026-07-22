export default defineI18nConfig(() => ({
  legacy: false,
  fallbackLocale: {
    'zh-HK': ['zh-TW', 'en', 'ja'],
    'zh-TW': ['en', 'ja'],
    'zh-CN': ['en', 'ja'],
    ko: ['en', 'ja'],
    en: ['ja'],
    ja: ['en'],
    default: ['en', 'ja'],
  },
  missingWarn: true,
  fallbackWarn: true,
}))
