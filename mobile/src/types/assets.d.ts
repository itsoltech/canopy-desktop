// Font assets imported inside DOM components ('use dom') resolve to URL
// strings via Metro's web asset pipeline. Declared here so `import fontUrl
// from '.../font.ttf'` typechecks.
declare module '*.ttf' {
  const src: string
  export default src
}

// React Native image imports. Metro returns an opaque asset source that
// React Native's Image component (and expo-image) understand — typed here
// as `number` to match the runtime shape.
declare module '*.png' {
  const src: number
  export default src
}
