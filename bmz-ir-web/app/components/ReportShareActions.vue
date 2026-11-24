<script setup lang="ts">
const props = defineProps<{
  captureTarget: HTMLElement | null
  sharePath: string
  shareText: string
  filename: string
}>()
const { t } = useI18n()
const toast = useToast()
const { origin } = useRequestURL()
const shareUrl = computed(() => new URL(props.sharePath, origin).href)
const xUrl = computed(() => {
  const query = new URLSearchParams({ text: props.shareText, url: shareUrl.value })
  return `https://x.com/intent/tweet?${query}`
})
const saving = ref(false)

async function copyUrl() {
  try {
    await navigator.clipboard.writeText(shareUrl.value)
    toast.add({ title: t('reportShare.copied'), color: 'success', icon: 'i-lucide-check' })
  } catch {
    toast.add({ title: t('reportShare.copyFailed'), color: 'error' })
  }
}

async function saveImage() {
  if (import.meta.server || saving.value || !props.captureTarget) return
  saving.value = true
  const target = props.captureTarget
  const filename = props.filename
  const preview = document.createElement('div')
  try {
    const clone = target.cloneNode(true) as HTMLElement
    const scroll = target.querySelector<HTMLElement>('[data-report-scroll]')
    const width = target.getBoundingClientRect().width
    // Give scrolling tables enough room for the chart titles and all score columns.
    const layoutWidth = scroll
      ? Math.max(1080, width + Math.max(0, scroll.scrollWidth - scroll.clientWidth))
      : width
    clone.style.width = `${Math.ceil(layoutWidth) + 32}px`
    clone.style.padding = '16px'
    clone.style.backgroundColor = getComputedStyle(document.body).backgroundColor
    clone.querySelectorAll('[data-report-controls]').forEach((element) => element.remove())
    preview.inert = true
    preview.setAttribute('aria-hidden', 'true')
    preview.style.cssText = 'position:fixed; top:0; left:-100000px'
    preview.append(clone)
    document.body.append(preview)

    const { snapdom } = await import('@zumer/snapdom')
    await snapdom.download(clone, { filename, format: 'png', fast: false })
  } catch {
    toast.add({ title: t('reportShare.imageFailed'), color: 'error' })
  } finally {
    preview.remove()
    saving.value = false
  }
}
</script>

<template>
  <div data-report-controls class="flex flex-wrap items-center gap-2">
    <UButton
      type="button"
      color="neutral"
      variant="subtle"
      size="sm"
      icon="i-lucide-camera"
      :loading="saving"
      :disabled="!captureTarget"
      :label="t('reportShare.saveImage')"
      @click="saveImage"
    />
    <UButton
      type="button"
      color="neutral"
      variant="subtle"
      size="sm"
      icon="i-lucide-link"
      :label="t('reportShare.copyUrl')"
      @click="copyUrl"
    />
    <UButton
      color="neutral"
      variant="subtle"
      size="sm"
      icon="i-lucide-external-link"
      :label="t('reportShare.postOnX')"
      :to="xUrl"
      target="_blank"
      rel="noopener noreferrer"
    />
  </div>
</template>
