import { mount } from 'svelte'
import '../../renderer-shared/styles/tokens.css'
import NotchOverlay from './components/notch/NotchOverlay.svelte'

const app = mount(NotchOverlay, {
  target: document.getElementById('app')!,
})

export default app
