// Font assets imported inside DOM components ('use dom') resolve to URL
// strings via Metro's web asset pipeline. Declared here so `import fontUrl
// from '.../font.ttf'` typechecks.
declare module '*.ttf' {
  const src: string
  export default src
}
