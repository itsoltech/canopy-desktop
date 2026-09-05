export function teamCityTokenCreationUrl(baseUrl: string): string {
  return `${baseUrl.replace(/\/$/, '')}/profile.html?item=accessTokens`
}
