import { createApp } from 'vue'
import DownloadsAnalyzeApp from './components/DownloadsAnalyzeApp.vue'

try {
  const app = createApp(DownloadsAnalyzeApp)
  app.mount('#app')
} catch (err) {
  console.error('[文件夹分析] Vue 挂载失败:', err)
  const appDiv = document.getElementById('app')
  if (appDiv) {
    appDiv.innerHTML = '<div style="color:red;padding:8px;font-size:13px;">Vue挂载失败: ' + String(err) + '</div>'
  }
}