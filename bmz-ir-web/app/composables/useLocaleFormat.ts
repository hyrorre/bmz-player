type DateInput = Date | string | number

export function useLocaleFormat() {
  const { locale } = useI18n()

  const localeTag = computed(() => {
    const tags: Record<string, string> = {
      ja: 'ja-JP',
      en: 'en-US',
      ko: 'ko-KR',
      'zh-CN': 'zh-CN',
      'zh-TW': 'zh-TW',
      'zh-HK': 'zh-HK',
    }
    return tags[locale.value] ?? 'ja-JP'
  })

  function formatNumber(value: number, options?: Intl.NumberFormatOptions) {
    return new Intl.NumberFormat(localeTag.value, options).format(value)
  }

  function formatDateTime(value: DateInput) {
    return new Intl.DateTimeFormat(localeTag.value, {
      dateStyle: 'medium',
      timeStyle: 'medium',
    }).format(new Date(value))
  }

  function formatDate(value: DateInput, options?: Intl.DateTimeFormatOptions) {
    return new Intl.DateTimeFormat(
      localeTag.value,
      options ?? { year: 'numeric', month: 'long', day: 'numeric', timeZone: 'Asia/Tokyo' },
    ).format(new Date(value))
  }

  function compareText(left: string, right: string) {
    return left.localeCompare(right, localeTag.value, { numeric: true })
  }

  return { localeTag, formatNumber, formatDateTime, formatDate, compareText }
}
