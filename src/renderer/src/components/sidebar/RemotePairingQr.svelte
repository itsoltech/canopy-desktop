<script lang="ts">
  import { onDestroy } from 'svelte'
  import QRCodeStyling from 'qr-code-styling'
  import appIconUrl from '../../assets/app-icon.png'

  interface Props {
    url: string
  }

  let { url }: Props = $props()

  let qrEl: HTMLDivElement | undefined = $state()
  let qrInstance: QRCodeStyling | null = null

  onDestroy(() => {
    // eslint-disable-next-line svelte/no-dom-manipulating
    qrEl?.replaceChildren()
    qrInstance = null
  })

  $effect(() => {
    if (!qrEl) return
    const style = getComputedStyle(document.documentElement)
    const textColor = style.getPropertyValue('--color-text').trim()
    const accentColor = style.getPropertyValue('--color-accent').trim()

    if (qrInstance) {
      qrInstance.update({ data: url })
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
