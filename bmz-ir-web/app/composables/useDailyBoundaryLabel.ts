/** 成果日の切り替わり時刻に、成果日の日付から見た前日・翌日を添える。 */
export function useDailyBoundaryLabel() {
  const { t } = useI18n()
  return (time: string, dayOffset: number) => {
    if (dayOffset < 0) return t('dailyBoundary.previousDay', { time })
    if (dayOffset > 0) return t('dailyBoundary.nextDay', { time })
    return time
  }
}
