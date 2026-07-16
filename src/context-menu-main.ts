import { createApp } from 'vue'
import ContextMenuApp from './ContextMenuApp.vue'
import './context-menu-style.css'

try {
  const app = createApp(ContextMenuApp)
  app.mount('#app')

  const banner = document.getElementById('debug-banner')
  if (banner) {
    banner.style.display = 'none'
  }
} catch (err) {
  const appDiv = document.getElementById('app')
  if (appDiv) {
    appDiv.innerHTML = '<div style="color:red;padding:8px;font-size:11px;">Vue挂载失败: ' + String(err) + '</div>'
  }
}
