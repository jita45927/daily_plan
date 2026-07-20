<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { useTaskStore } from '../stores/taskStore'
import { invoke } from '@tauri-apps/api/core'

const taskStore = useTaskStore()

const menuStyle = computed(() => {
  const x = taskStore.mainMenu.x
  const y = taskStore.mainMenu.y
  const maxX = window.innerWidth - 160
  const maxY = window.innerHeight - 100
  return {
    left: `${Math.min(x, maxX)}px`,
    top: `${Math.min(y, maxY)}px`
  }
})

const handleCleanComputer = () => {
  taskStore.closeMainMenu()
}

const handleOrganizeDesktop = async () => {
  taskStore.closeMainMenu()
  // 第一步：显示 loading，后台执行分析（不创建窗口，避免焦点转移导致主窗口收起）
  taskStore.isAnalyzingDesktop = true
  try {
    await invoke('analyze_desktop_cmd')
  } catch (error: any) {
    console.error('分析桌面失败:', error)
    alert('分析桌面失败:\n' + (error?.message || error?.toString() || '未知错误'))
    return
  } finally {
    taskStore.isAnalyzingDesktop = false
  }
  // 第二步：分析完成后，创建并显示结果窗口
  try {
    await invoke('show_analyze_window')
  } catch (error: any) {
    console.error('显示分析窗口失败:', error)
    alert('显示分析窗口失败:\n' + (error?.message || error?.toString() || '未知错误'))
  }
}

const handleCleanDuplicateFiles = () => {
  taskStore.closeMainMenu()
}

const handleContextMenu = (e: MouseEvent) => {
  if (taskStore.mainMenu.show) {
    e.preventDefault()
    taskStore.closeMainMenu()
  }
}

onMounted(() => {
  document.addEventListener('contextmenu', handleContextMenu)
})

onUnmounted(() => {
  document.removeEventListener('contextmenu', handleContextMenu)
})
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="taskStore.mainMenu.show"
        class="main-menu fixed z-50 bg-white rounded-lg shadow-xl overflow-hidden"
        :style="menuStyle"
      >
        <div class="p-2">
          <button
            @click="handleCleanComputer"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-blue-50 rounded transition-colors"
          >
            清理电脑
          </button>
          <button
            @click="handleOrganizeDesktop"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-green-50 rounded transition-colors"
          >
            整理桌面
          </button>
          <button
            @click="handleCleanDuplicateFiles"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-orange-50 rounded transition-colors"
          >
            清理重复文件
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>