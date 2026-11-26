type DateInput = Date | string | number

const dateAndTimeFormats = new Map<
  string,
  { date: Intl.DateTimeFormat; time: Intl.DateTimeFormat }
>()

export function useLocaleFormat() {
  const { locale } = useI18n()
  const timeZone = useViewerTimeZone()

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
      timeZone: timeZone.value,
    }).format(new Date(value))
  }

  /** 閲覧者のタイムゾーンの日付。options の timeZone で別のタイムゾーンを指定できる。 */
  function formatDate(value: DateInput, options?: Intl.DateTimeFormatOptions) {
    return new Intl.DateTimeFormat(localeTag.value, {
      timeZone: timeZone.value,
      ...(options ?? { year: 'numeric', month: 'long', day: 'numeric' }),
    }).format(new Date(value))
  }

  /** 閲覧者のタイムゾーンの時刻を、タイムゾーンの略称付きで表示する。 */
  function formatTimeWithZone(value: DateInput) {
    return new Intl.DateTimeFormat(localeTag.value, {
      hour: 'numeric',
      minute: '2-digit',
      timeZoneName: 'short',
      timeZone: timeZone.value,
    }).format(new Date(value))
  }

  function formatDateAndTime(value: DateInput) {
    const locale = localeTag.value
    const key = `${locale}\u0000${timeZone.value}`
    let formats = dateAndTimeFormats.get(key)
    if (!formats) {
      formats = {
        date: new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeZone: timeZone.value }),
        time: new Intl.DateTimeFormat(locale, { timeStyle: 'medium', timeZone: timeZone.value }),
      }
      dateAndTimeFormats.set(key, formats)
    }
    const timestamp = new Date(value)
    return { date: formats.date.format(timestamp), time: formats.time.format(timestamp) }
  }

  function compareText(left: string, right: string) {
    return left.localeCompare(right, localeTag.value, { numeric: true })
  }

  return {
    localeTag,
    timeZone,
    formatNumber,
    formatDateTime,
    formatDate,
    formatTimeWithZone,
    formatDateAndTime,
    compareText,
  }
}
