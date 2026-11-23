<script setup lang="ts">
const localePath = useLocalePath()
</script>

<template>
  <UFooter class="pt-8 pb-4">
    <div lang="en" class="text-center text-sm">
      <p>BMZ Internet Ranking - made by hyrorre</p>
      <p class="mt-2 flex flex-wrap justify-center gap-x-4 gap-y-1">
        <ULink :to="localePath('/licenses')">Licenses</ULink>
        <ULink to="https://github.com/hyrorre/bmz-player" target="_blank">GitHub</ULink>
        <ULink to="https://x.com/hyrorre" target="_blank">Twitter</ULink>
      </p>
    </div>
  </UFooter>
</template>
