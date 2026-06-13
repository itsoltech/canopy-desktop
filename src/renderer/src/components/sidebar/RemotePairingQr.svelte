<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import QRCodeStyling from 'qr-code-styling'
  import appIconUrl from '../../assets/app-icon.png'

  interface Props {
    url: string
  }

  let { url }: Props = $props()

  let qrEl: HTMLDivElement | undefined = $state()
  let qrInstance: QRCodeStyling | null = null
  let themeObserver: MutationObserver | null = null

  onDestroy(() => {
    themeObserver?.disconnect()
    themeObserver = null
    // eslint-disable-next-line svelte/no-dom-manipulating
    qrEl?.replaceChildren()
    qrInstance = null
  })

  onMount(() => {
    themeObserver = new MutationObserver(() => updateQr())
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'style'],
    })
  })

  function getQrColors(): { textColor: string; accentColor: string } {
    const style = getComputedStyle(document.documentElement)
    return {
      textColor: style.getPropertyValue('--color-text').trim(),
      accentColor: style.getPropertyValue('--color-accent').trim(),
    }
  }

  function updateQr(): void {
    if (!qrInstance) return
    const { textColor, accentColor } = getQrColors()
    qrInstance.update({
      data: url,
      dotsOptions: { color: textColor, type: 'rounded' },
      cornersSquareOptions: { color: accentColor, type: 'extra-rounded' },
      cornersDotOptions: { color: accentColor },
    })
  }

  $effect(() => {
    if (!qrEl) return
    const { textColor, accentColor } = getQrColors()
    if (qrInstance) {
      updateQr()
      return
    }

    qrInstance = new QRCodeStyling({
      width: 168,
      height: 168,
      type: 'svg',
      data: url,
      image: appIconUrl,
      dotsOptions: { color: textColor, type: 'rounded' },
      backgroundOptions: { color: 'transparent' },
      cornersSquareOptions: { color: accentColor, type: 'extra-rounded' },
      cornersDotOptions: { color: accentColor },
      qrOptions: { errorCorrectionLevel: 'H' },
      imageOptions: {
        crossOrigin: 'anonymous',
        imageSize: 0.18,
        margin: 3,
        hideBackgroundDots: true,
      },
    })
    qrInstance.append(qrEl)
  })
</script>

<div class="[&_svg]:block" bind:this={qrEl}></div>
