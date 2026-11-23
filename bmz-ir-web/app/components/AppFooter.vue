<script setup lang="ts">
const localePath = useLocalePath()
</script>

<template>
  <UFooter class="pt-8 pb-4">
    <div
      lang="en"
      class="flex flex-wrap items-center justify-center gap-x-4 gap-y-1 text-center text-sm"
    >
      <p>BMZ Internet Ranking - made by hyrorre</p>
      <ULink :to="localePath('/licenses')" class="underline underline-offset-4">Licenses</ULink>
    </div>
  </UFooter>
</template>
