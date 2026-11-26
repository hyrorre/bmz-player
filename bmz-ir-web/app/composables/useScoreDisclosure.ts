export function useScoreDisclosure() {
  const expandedScoreRow = ref<string | null>(null)
  const scoreDetailsId = useId()

  function toggleScoreDetails(row: string) {
    expandedScoreRow.value = expandedScoreRow.value === row ? null : row
  }

  return { expandedScoreRow, scoreDetailsId, toggleScoreDetails }
}

/** Expanded rows must span the columns shown at Tailwind's xl breakpoint. */
export function useWideScoreTable() {
  return useViewportMatch('(min-width: 80rem)')
}
