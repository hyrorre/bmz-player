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
const imageAction = ref<'save' | 'copy' | null>(null)

async function copyUrl() {
  try {
    await navigator.clipboard.writeText(shareUrl.value)
    toast.add({ title: t('reportShare.copied'), color: 'success', icon: 'i-lucide-check' })
  } catch {
    toast.add({ title: t('reportShare.copyFailed'), color: 'error' })
  }
}

async function captureReport(target: HTMLElement) {
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
    return await snapdom(clone, { format: 'png', fast: false })
  } finally {
    preview.remove()
  }
}

async function copyReportImage(target: HTMLElement) {
  if (typeof ClipboardItem === 'undefined' || !navigator.clipboard?.write) {
    throw new Error('Image clipboard is unavailable')
  }
  const png = captureReport(target).then((capture) => capture.toBlob({ format: 'png', dpr: 1 }))
  const captureFinished = png.catch(() => undefined)
  try {
    // Start the clipboard write during the click, before the asynchronous PNG export completes.
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })])
  } finally {
    await captureFinished
  }
}

async function exportImage(action: 'save' | 'copy') {
  if (import.meta.server || imageAction.value || !props.captureTarget) return
  imageAction.value = action
  try {
    if (action === 'copy') {
      await copyReportImage(props.captureTarget)
      toast.add({ title: t('reportShare.imageCopied'), color: 'success', icon: 'i-lucide-check' })
    } else {
      const filename = props.filename
      const capture = await captureReport(props.captureTarget)
      await capture.download({ filename, format: 'png' })
    }
  } catch {
    if (action === 'copy') {
      toast.add({ title: t('reportShare.imageCopyFailed'), color: 'error' })
    } else {
      toast.add({ title: t('reportShare.imageFailed'), color: 'error' })
    }
  } finally {
    imageAction.value = null
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
      :loading="imageAction === 'save'"
      :disabled="!captureTarget || imageAction !== null"
      :label="t('reportShare.saveImage')"
      @click="exportImage('save')"
    />
    <UButton
      type="button"
      color="neutral"
      variant="subtle"
      size="sm"
      icon="i-lucide-copy"
      :loading="imageAction === 'copy'"
      :disabled="!captureTarget || imageAction !== null"
      :label="t('reportShare.copyImage')"
      @click="exportImage('copy')"
    />
    <UButton
      type="button"
      color="neutral"
      variant="subtle"
      size="sm"
      icon="i-lucide-link"
      :disabled="imageAction === 'copy'"
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
