export function useViewportMatch(query: string) {
  const matches = ref(false)
  let media: MediaQueryList | undefined

  function update(event: MediaQueryListEvent) {
    matches.value = event.matches
  }

  onMounted(() => {
    media = window.matchMedia(query)
    matches.value = media.matches
    media.addEventListener('change', update)
  })
  onBeforeUnmount(() => media?.removeEventListener('change', update))

  return matches
}
