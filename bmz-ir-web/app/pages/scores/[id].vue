<script setup lang="ts">
definePageMeta({
  middleware: defineNuxtRouteMiddleware(async (to) => {
    const localePath = useLocalePath()
    const scoreId = encodeURIComponent(String(to.params.id))
    const { score } = await useRequestFetch()<{ score: { chart_sha256: string } }>(
      `/api/v1/scores/${scoreId}`,
    )
    return navigateTo(localePath({ path: `/charts/${score.chart_sha256}`, query: to.query }), {
      replace: true,
    })
  }),
})
</script>

<template>
  <div />
</template>
