<script lang="ts">
  // Brand marks for the supported trackers and CI providers. Jira and GitHub come from
  // simple-icons (CC0); YouTrack is JetBrains' official product icon and TeamCity a
  // simplified referential rendition of it (their brand assets permit referential use).
  let { provider, size = 12 }: { provider: string; size?: number } = $props()
</script>

{#if provider === 'jira'}
  <svg width={size} height={size} viewBox="0 0 24 24" fill="#2684FF" aria-hidden="true">
    <path
      d="M11.571 11.513H0a5.218 5.218 0 0 0 5.232 5.215h2.13v2.057A5.215 5.215 0 0 0 12.575 24V12.518a1.005 1.005 0 0 0-1.005-1.005zm5.723-5.756H5.736a5.215 5.215 0 0 0 5.215 5.214h2.129v2.058a5.218 5.218 0 0 0 5.215 5.214V6.758a1.001 1.001 0 0 0-1.001-1.001zM23.013 0H11.455a5.215 5.215 0 0 0 5.215 5.215h2.129v2.057A5.215 5.215 0 0 0 24 12.483V1.005A1.001 1.001 0 0 0 23.013 0Z"
    />
  </svg>
{:else if provider === 'youtrack'}
  <svg width={size} height={size} viewBox="0 0 64 64" fill="none" aria-hidden="true">
    <defs>
      <linearGradient
        id="yt-brand-grad"
        x1="-.102411"
        x2="64.0532"
        y1="32.0002"
        y2="32.0002"
        gradientUnits="userSpaceOnUse"
      >
        <stop stop-color="#FB43FF" />
        <stop offset=".97" stop-color="#FB406D" />
      </linearGradient>
    </defs>
    <path
      fill="url(#yt-brand-grad)"
      d="M1.30634 51.2446c-.09688-.0713-.12875-.2019-.07625-.31L8.86571 35.209.058214 24.6908c-.08875-.1056-.074375-.2637.031875-.3518L25.7788 2.93209C30.3495-.877908 36.967-.985408 41.6626 2.67147c4.6937 3.65687 6.1937 10.08813 3.5969 15.43623l-2.7994 5.7669c1.0919-.3663 2.1725-.6994 3.2412-.9988l12.6738-3.6406c.1381-.04.2819.045.3131.1856l5.3056 23.585c.0325.1457-.0662.2882-.215.3069-1.6818.2119-10.8575 1.53-22.2812 6.3294-12.9431 5.4362-21.4844 13.1625-22.6944 14.2925-.0894.0837-.2206.0869-.3187.0144L1.30634 51.2446Z"
    />
    <path fill="#000" d="M52 12H12v40h40V12Z" />
    <path
      fill="#fff"
      d="m21.4666 26.3709-5.4881-9.3787h3.1513l3.3981 5.9919.3969.8575.3968-.8682 3.3119-5.9812h3.0975l-5.4025 9.3575v5.6487h-2.8619v-5.6275Z"
    />
    <path fill="#fff" d="M33 43.9984H17v3h16v-3Z" />
    <path
      fill="#fff"
      d="M42.3248 16.9922H30.2879l-.0006 2.6369h4.5662v12.3693h2.9263V19.6291h4.545v-2.6369Z"
    />
  </svg>
{:else if provider === 'teamcity'}
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true">
    <defs>
      <linearGradient
        id="tc-brand-grad"
        x1="0"
        y1="0"
        x2="24"
        y2="24"
        gradientUnits="userSpaceOnUse"
      >
        <stop stop-color="#0CB0F2" />
        <stop offset="1" stop-color="#905CFB" />
      </linearGradient>
    </defs>
    <rect width="24" height="24" rx="3" fill="url(#tc-brand-grad)" />
    <path fill="#fff" d="M4.5 7h7.2v2.1H8.95V17h-1.9V9.1H4.5V7Z" />
    <path
      d="M19.4 9.6a3.7 3.7 0 1 0 0 4.8"
      stroke="#fff"
      stroke-width="1.9"
      fill="none"
      stroke-linecap="round"
    />
  </svg>
{:else if provider === 'github'}
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path
      d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"
    />
  </svg>
{/if}
