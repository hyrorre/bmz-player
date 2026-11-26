/** 成果日の切り替わり時刻に、成果日の日付から見た現地の日差を添える。 */
export function useDailyBoundaryLabel() {
  const { t } = useI18n()
  return (time: string, dayOffset: number) => {
    if (dayOffset === 0) return time
    if (dayOffset === -1) return t('dailyBoundary.previousDay', { time })
    if (dayOffset === 1) return t('dailyBoundary.nextDay', { time })
    return t(dayOffset < 0 ? 'dailyBoundary.daysBefore' : 'dailyBoundary.daysAfter', {
      days: Math.abs(dayOffset),
      time,
    })
  }
}
