export function useScoreDisclosure() {
  const expandedScoreRow = ref<string | null>(null)
  const scoreDetailsId = useId()

  function toggleScoreDetails(row: string) {
    expandedScoreRow.value = expandedScoreRow.value === row ? null : row
  }

  return { expandedScoreRow, scoreDetailsId, toggleScoreDetails }
}
