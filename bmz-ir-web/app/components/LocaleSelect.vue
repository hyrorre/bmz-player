<script setup lang="ts">
const { locale, locales, setLocale, t } = useI18n()
type LocaleCode = 'ja' | 'en' | 'ko' | 'zh-CN' | 'zh-TW' | 'zh-HK'

const localeItems = computed(() =>
  locales.value.map((item) => ({
    label: typeof item === 'string' ? item : (item.name ?? item.code),
    value: (typeof item === 'string' ? item : item.code) as LocaleCode,
  })),
)

async function switchLocale(value: LocaleCode) {
  await setLocale(value)
}
</script>

<template>
  <USelect
    :aria-label="t('common.language')"
    class="w-40"
    :items="localeItems"
    :model-value="locale"
    size="sm"
    @update:model-value="switchLocale"
  />
</template>
