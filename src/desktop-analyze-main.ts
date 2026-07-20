import { createApp } from 'vue'
import DesktopAnalyzeApp from './components/DesktopAnalyzeApp.vue'

try {
  const app = createApp(DesktopAnalyzeApp)
  app.mount('#app')
} catch (err) {
  console.error('[桌面分析] Vue 挂载失败:', err)
  const appDiv = document.getElementById('app')
  if (appDiv) {
    appDiv.innerHTML = '<div style="color:red;padding:8px;font-size:13px;">Vue挂载失败: ' + String(err) + '</div>'
  }
}