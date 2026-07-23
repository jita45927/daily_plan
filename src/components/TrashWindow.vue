<script setup lang="ts">
import { useTaskStore, type DeletedTask } from '../stores/taskStore'
import { invoke } from '@tauri-apps/api/core'

const taskStore = useTaskStore()

const handleClearAll = () => {
  taskStore.showConfirm('清理回收站', '确定要清空回收站吗？所有任务将被彻底删除！', () => {
    taskStore.clearTrashByPeriod(0)
  })
}

const formatDeletedAt = (dateStr: string) => {
  const d = new Date(dateStr)
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  const h = String(d.getHours()).padStart(2, '0')
  const min = String(d.getMinutes()).padStart(2, '0')
  return `${y}-${m}-${day} ${h}:${min}`
}

const taskTextStyle = (task: DeletedTask) => {
  const styles: Record<string, string> = {}
  if (task.status) {
    styles.color = '#6B7280'
    styles.fontWeight = 'normal'
  } else {
    styles.color = task.color || '#000000'
    styles.fontWeight = task.bold ? 'bold' : 'normal'
  }
  return styles
}

const handleContextMenu = (e: MouseEvent, task: DeletedTask) => {
  e.preventDefault()
  e.stopPropagation()
  invoke('show_trash_context_menu', {
    screenX: e.screenX,
    screenY: e.screenY,
    taskId: task.id
  }).catch(err => {
    console.error('show_trash_context_menu error:', err)
  })
}

const handleWindowMouseDown = () => {
  invoke('close_trash_context_menu').catch(() => {})
}
</script>

<template>
  <div
    v-if="taskStore.trashWindowVisible"
    class="trash-window-content no-drag absolute inset-0 flex flex-col bg-yellow-200 overflow-hidden z-40"
    @mousedown.stop="handleWindowMouseDown"
  >
    <div class="flex items-center justify-between px-4 py-3 border-b border-yellow-500/30 flex-shrink-0">
      <h2 class="text-lg font-bold text-gray-800">回收站</h2>
      <button
        @click="taskStore.closeTrashWindow()"
        class="w-8 h-8 rounded-full hover:bg-yellow-500/50 flex items-center justify-center transition-colors"
      >
        <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-gray-700" viewBox="0 0 20 20" fill="currentColor">
          <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
        </svg>
      </button>
    </div>

    <div class="flex-1 overflow-y-auto px-2 py-2">
      <div v-if="taskStore.deletedTasks.length === 0" class="flex items-center justify-center h-full text-gray-600 text-sm">
        回收站为空
      </div>
      <div
        v-for="task in taskStore.deletedTasks"
        :key="task.id"
        class="trash-task-item px-3 py-2 cursor-default border-b border-yellow-500/20 hover:bg-yellow-300/50"
        @contextmenu="handleContextMenu($event, task)"
      >
        <div class="flex items-start gap-2">
          <div class="flex-shrink-0 w-5 h-5 flex items-center justify-center pt-0.5">
            <div
              v-if="!task.status && !task.bold"
              class="w-2.5 h-2.5 rounded-full bg-gray-600/60"
            ></div>
            <span
              v-else-if="task.status"
              class="text-green-600 font-bold text-sm"
            >✓</span>
            <span
              v-else-if="!task.status && task.bold"
              class="text-red-500 font-bold text-sm"
            >✕</span>
          </div>

          <div class="flex-1 min-w-0">
            <p
              class="leading-snug break-words"
              :class="[task.status ? 'line-through' : '']"
              :style="{ ...taskTextStyle(task), fontSize: '14px' }"
            >
              {{ task.text }}
              <span class="text-gray-500 font-normal ml-1" style="font-size: 11px;">
                删除于 {{ formatDeletedAt(task.deletedAt) }}
              </span>
            </p>
          </div>
        </div>
      </div>
    </div>

    <div class="px-3 py-2 border-t border-yellow-500/30 flex-shrink-0">
      <button
        @click="handleClearAll"
        class="w-full py-2 bg-red-500 text-white rounded-lg hover:bg-red-600 transition-colors text-sm font-medium"
      >
        清理全部
      </button>
    </div>
  </div>
</template>
