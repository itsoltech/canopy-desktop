// electron-vite HMR client reads window.electron which does not exist
// when remote.html is served over HTTP to a phone/tablet.
if (!window.electron) window.electron = {}
