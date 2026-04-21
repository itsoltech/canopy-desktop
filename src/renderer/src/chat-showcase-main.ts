import { mount } from 'svelte'

import './assets/main.css'

import ChatShowcase from './ChatShowcase.svelte'

const app = mount(ChatShowcase, {
  target: document.getElementById('app')!,
})

export default app
