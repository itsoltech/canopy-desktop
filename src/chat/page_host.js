// Only this trusted outer document participates in the native page lifecycle.
(() => {
  let ready;
  window.__canopyReady = new Promise(resolve => { ready = resolve; });
  window.addEventListener('message', event => {
    const frame = document.querySelector('iframe');
    if (!frame || event.source !== frame.contentWindow || !event.data) return;
    const {canopyPage, value} = event.data;
    if (canopyPage === 'height' && typeof value === 'number'
        && Number.isFinite(value) && value > 0) {
      frame.style.height = Math.min(1000000, Math.ceil(value)) + 'px';
    } else if (canopyPage === 'ready' && value === true) {
      ready();
    }
  });
})();
