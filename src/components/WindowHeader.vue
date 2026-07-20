<script setup lang="ts">
import { useTaskStore } from '../stores/taskStore'

const taskStore = useTaskStore()

const handleMenuClick = (e: MouseEvent) => {
  e.stopPropagation()
  const target = e.currentTarget as HTMLElement
  const rect = target.getBoundingClientRect()
  taskStore.closeContextMenu()
  taskStore.openMainMenu(rect.left, rect.bottom)
}
</script>

<template>
  <div class="flex items-center justify-between px-4 py-3">
    <button
      @click="handleMenuClick"
      class="w-10 h-10 rounded-full bg-yellow-400 hover:bg-yellow-500 flex items-center justify-center transition-colors shadow-md no-drag"
      title="菜单"
    >
      <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-gray-800" viewBox="0 0 20 20" fill="currentColor">
        <path d="M3 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z" />
      </svg>
    </button>
    
    <div class="text-sm font-medium text-gray-800">
      {{ new Date().toLocaleDateString('zh-CN', { weekday: 'long', month: 'long', day: 'numeric' }) }}
    </div>
    
    <button
      @click="taskStore.toggleWindowLock"
      :class="[
        'w-10 h-10 rounded-full flex items-center justify-center transition-colors shadow-md',
        taskStore.isWindowLocked ? 'bg-yellow-500' : 'bg-yellow-400 hover:bg-yellow-500'
      ]"
      :title="taskStore.isWindowLocked ? '解锁（恢复收起）' : '固定窗口（禁用收起）'"
    >
      <svg v-if="!taskStore.isWindowLocked" xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-gray-800" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M5 9V7a5 5 0 0110 0v2a2 2 0 012 2v5a2 2 0 01-2 2H5a2 2 0 01-2-2v-5a2 2 0 012-2zm8-2v2H7V7a3 3 0 016 0z" clip-rule="evenodd" />
      </svg>
      <svg v-else xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-gray-800" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M12.395 2.553a1 1 0 00-1.45-.385c-.345.23-.614.558-.822.88-.214.33-.403.713-.57 1.116-.334.804-.614 1.768-.84 2.734a31.365 31.365 0 00-.613 3.58 2.64 2.64 0 01-.945-1.067c-.328-.68-.398-1.534-.398-2.654A1 1 0 005.05 6.05 6.981 6.981 0 003 11a7 7 0 1011.95-4.95c-.592-.591-.98-.985-1.348-1.467-.363-.476-.724-1.063-1.207-2.03zM12.12 15.12A3 3 0 017 13s.879.5 2.5.5c0-1 .5-4 1.25-4.5.5 1 .786 1.293 1.371 1.879A2.99 2.99 0 0113 13a2.99 2.99 0 01-.879 2.121z" clip-rule="evenodd" />
      </svg>
    </button>
  </div>
</template>
