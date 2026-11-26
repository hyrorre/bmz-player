<script setup lang="ts">
definePageMeta({
  middleware: defineNuxtRouteMiddleware(async (to) => {
    const localePath = useLocalePath()
    // Visibility can change when a viewer signs out or a score is moderated.
    if (import.meta.server) useResponseHeader('Cache-Control').value = 'private, no-store'
    const scoreId = encodeURIComponent(String(to.params.id))
    const { score } = await useRequestFetch()<{ score: { chart_sha256: string } }>(
      `/api/v1/scores/${scoreId}`,
    )
    return navigateTo(localePath({ path: `/charts/${score.chart_sha256}`, query: to.query }), {
      redirectCode: 301,
      replace: true,
    })
  }),
})
</script>

<template>
  <div />
</template>
