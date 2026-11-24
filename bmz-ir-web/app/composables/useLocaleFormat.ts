type DateInput = Date | string | number

const timeZone = 'Asia/Tokyo'

const dateAndTimeFormats = new Map<
  string,
  { date: Intl.DateTimeFormat; time: Intl.DateTimeFormat }
>()

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
      timeZone,
    }).format(new Date(value))
  }

  function formatDate(value: DateInput, options?: Intl.DateTimeFormatOptions) {
    return new Intl.DateTimeFormat(localeTag.value, {
      timeZone,
      ...(options ?? { year: 'numeric', month: 'long', day: 'numeric' }),
    }).format(new Date(value))
  }

  function formatDateAndTime(value: DateInput) {
    const locale = localeTag.value
    let formats = dateAndTimeFormats.get(locale)
    if (!formats) {
      formats = {
        date: new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeZone }),
        time: new Intl.DateTimeFormat(locale, { timeStyle: 'medium', timeZone }),
      }
      dateAndTimeFormats.set(locale, formats)
    }
    const timestamp = new Date(value)
    return { date: formats.date.format(timestamp), time: formats.time.format(timestamp) }
  }

  function compareText(left: string, right: string) {
    return left.localeCompare(right, localeTag.value, { numeric: true })
  }

  return { localeTag, formatNumber, formatDateTime, formatDate, formatDateAndTime, compareText }
}
