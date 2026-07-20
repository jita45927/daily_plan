console.log('[桌面分析] desktop-analyze-main.ts 开始执行')

import { createApp } from 'vue'
import DesktopAnalyzeApp from './components/DesktopAnalyzeApp.vue'

console.log('[桌面分析] Vue 和组件导入成功')

try {
  console.log('[桌面分析] 获取 #app 元素...')
  const appDiv = document.getElementById('app')
  console.log('[桌面分析] #app 元素:', appDiv)
  
  if (appDiv) {
    console.log('[桌面分析] 创建 Vue 应用...')
    const app = createApp(DesktopAnalyzeApp)
    console.log('[桌面分析] 挂载 Vue 应用...')
    app.mount('#app')
    console.log('[桌面分析] Vue 应用挂载成功')
  } else {
    throw new Error('#app 元素不存在')
  }
} catch (err) {
  console.error('[桌面分析] Vue 挂载失败:', err)
  const appDiv = document.getElementById('app')
  if (appDiv) {
    appDiv.innerHTML = '<div style="color:red;padding:8px;font-size:13px;">Vue挂载失败: ' + String(err) + '</div>'
  }
}