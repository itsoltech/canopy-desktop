// Runs inside the opaque-origin sandbox. It never receives app APIs or credentials.
(() => {
  const post = (kind, value) => parent.postMessage({canopyPage: kind, value}, '*');
  let last = -1;
  const height = () => {
    const body = document.body;
    if (!body) return;
    const style = getComputedStyle(body);
    const value = Math.ceil(body.getBoundingClientRect().height
      + (parseFloat(style.marginTop) || 0) + (parseFloat(style.marginBottom) || 0));
    if (Number.isFinite(value) && value !== last) {
      last = value;
      post('height', Math.max(1, value));
    }
  };
  const watch = () => {
    height();
    const observer = new ResizeObserver(height);
    observer.observe(document.documentElement);
    if (document.body) observer.observe(document.body);
  };
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', watch, {once: true});
  } else {
    watch();
  }
  // Parent load includes srcdoc loading. Wait for the child's fonts and finite
  // animations too, just as the native local-page snapshot does.
  window.addEventListener('load', async () => {
    await Promise.race([
      document.fonts ? document.fonts.ready : Promise.resolve(),
      new Promise(resolve => setTimeout(resolve, 1500)),
    ]);
    await Promise.race([
      Promise.all((document.getAnimations ? document.getAnimations() : [])
        .map(animation => animation.finished.catch(() => {}))),
      new Promise(resolve => setTimeout(resolve, 1500)),
    ]);
    height();
    post('ready', true);
  }, {once: true});
})();
