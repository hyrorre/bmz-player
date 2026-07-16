<template>
  <UFooter class="pt-8 pb-4">
    <div class="text-center text-sm">
      <p>BMZ Internet Ranking — Built for the BMS community.</p>
      <p class="mt-1">BMS楽曲・譜面などの権利は、それぞれの制作者に帰属します。</p>
      <p class="mt-2 flex justify-center gap-4">
        <u-link :to="localePath('/licenses')">{{ t('common.licenses') }}</u-link>
        <u-link to="https://github.com/hyrorre/bmz-player" target="_blank">GitHub</u-link>
        <u-link to="https://x.com/hyrorre" target="_blank">Twitter</u-link>
      </p>
    </div>
  </UFooter>
</template>

<script setup lang="ts">
const localePath = useLocalePath()
const { t } = useI18n()
</script>
