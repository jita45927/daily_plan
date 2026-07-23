<script setup lang="ts">
import { ref, nextTick, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTaskStore } from '../stores/taskStore'

const taskStore = useTaskStore()
const newTask = ref('')
const showInput = ref(false)
const inputRef = ref<HTMLInputElement | null>(null)

const handleAdd = () => {
  if (newTask.value.trim()) {
    taskStore.addTask(newTask.value.trim())
    newTask.value = ''
    showInput.value = false
  }
}

const cancelAdd = () => {
  newTask.value = ''
  showInput.value = false
}

const focusInput = async () => {
  await nextTick()
  inputRef.value?.focus()
}

const handleResetApp = () => {
  taskStore.showConfirm('重置程序', '确定要重置程序吗？所有任务将被删除，窗口将恢复到初始位置。', async () => {
    try {
      // 先隐藏确认对话框，避免resetAllState重置时产生冲突
      taskStore.hideConfirm()
      await invoke('reset_app_cmd')
      // 重置所有状态（不包括confirmDialog，因为已经隐藏了）
      taskStore.tasks = []
      taskStore.deletedTasks = []
      taskStore.timerStates.clear()
      taskStore.isWindowLocked = false
      taskStore.trashWindowVisible = false
      // 重新加载回收站数据，确保与新数据库同步
      await taskStore.loadDeletedTasks()
    } catch (error) {
      console.error('Failed to reset app:', error)
      taskStore.hideConfirm()
    }
  })
}

const handleExitApp = () => {
  taskStore.showConfirm('退出程序', '确定要退出程序吗？', () => {
    invoke('exit_app')
  })
}

const handleClickOutside = (e: MouseEvent) => {
  const target = e.target as Node
  const isClickInside = document.querySelector('.add-task-container')?.contains(target)
  if (!isClickInside && showInput.value) {
    if (newTask.value.trim()) {
      handleAdd()
    } else {
      cancelAdd()
    }
  }
}

onMounted(() => {
  document.addEventListener('mousedown', handleClickOutside)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', handleClickOutside)
})
</script>

<template>
  <div class="add-task-container flex items-center justify-center gap-3 p-4">
    <div v-if="showInput" class="flex-1">
      <input
        id="add-task-input"
        name="add-task-input"
        ref="inputRef"
        v-model="newTask"
        @keyup.enter="handleAdd"
        @keyup.escape="cancelAdd"
        type="text"
        placeholder="添加新任务..."
        class="w-full px-4 py-3 bg-white/90 rounded-lg focus:outline-none focus:ring-2 focus:ring-yellow-500 text-gray-800 text-sm shadow-sm"
        autofocus
      />
    </div>
    <button
      v-if="!showInput"
      @click="showInput = true; focusInput()"
      class="w-6 h-6 rounded-full bg-yellow-500 hover:bg-yellow-600 flex items-center justify-center transition-all shadow-md hover:shadow-lg active:scale-95 flex-shrink-0"
      title="添加任务"
    >
      <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M10 3a1 1 0 011 1v5h5a1 1 0 110 2h-5v5a1 1 0 11-2 0v-5H4a1 1 0 110-2h5V4a1 1 0 011-1z" clip-rule="evenodd" />
      </svg>
    </button>
    <button
      v-if="!showInput"
      @click="taskStore.openTrashWindow()"
      class="w-6 h-6 rounded-full bg-yellow-500 hover:bg-yellow-600 flex items-center justify-center transition-all shadow-md hover:shadow-lg active:scale-95 flex-shrink-0"
      title="回收站"
    >
      <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
      </svg>
    </button>
    <button
      v-if="!showInput"
      @click="taskStore.toggleMute()"
      :class="[
        'w-6 h-6 rounded-full flex items-center justify-center transition-all shadow-md hover:shadow-lg active:scale-95 flex-shrink-0',
        taskStore.isMuted ? 'bg-red-500 hover:bg-red-600' : 'bg-yellow-500 hover:bg-yellow-600'
      ]"
      :title="taskStore.isMuted ? '静音（点击开启闹钟）' : '闹钟（点击关闭声音）'"
    >
      <svg v-if="!taskStore.isMuted" xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M9.383 3.076A1 1 0 0110 4v12a1 1 0 01-1.707.707L4.586 13H2a1 1 0 01-1-1V8a1 1 0 011-1h2.586l3.707-3.707a1 1 0 011.09-.217zM14.657 2.929a1 1 0 011.414 0A9.972 9.972 0 0119 10a9.972 9.972 0 01-2.929 7.071a1 1 0 01-1.414-1.414A7.971 7.971 0 0017 10c0-2.21-.894-4.208-2.343-5.657a1 1 0 010-1.414zm-2.829 2.828a1 1 0 011.415 0A5.983 5.983 0 0115 10a5.984 5.984 0 01-1.757 4.243a1 1 0 01-1.415-1.415A3.984 3.984 0 0013 10a3.983 3.983 0 00-1.172-2.828a1 1 0 010-1.415z" clip-rule="evenodd" />
      </svg>
      <svg v-else xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M9.383 3.076A1 1 0 0110 4v12a1 1 0 01-1.707.707L4.586 13H2a1 1 0 01-1-1V8a1 1 0 011-1h2.586l3.707-3.707a1 1 0 011.09-.217zM14.657 2.929a1 1 0 011.414 0A9.972 9.972 0 0119 10a9.972 9.972 0 01-2.929 7.071a1 1 0 01-1.414-1.414A7.971 7.971 0 0017 10c0-2.21-.894-4.208-2.343-5.657a1 1 0 010-1.414zm-2.829 2.828a1 1 0 011.415 0A5.983 5.983 0 0115 10a5.984 5.984 0 01-1.757 4.243a1 1 0 01-1.415-1.415A3.984 3.984 0 0013 10a3.983 3.983 0 00-1.172-2.828a1 1 0 010-1.415z" clip-rule="evenodd" />
        <path d="M11 6a1 1 0 011 1v3a1 1 0 01-2 0V7a1 1 0 011-1zm4 0a1 1 0 011 1v3a1 1 0 01-2 0V7a1 1 0 011-1z" />
      </svg>
    </button>
    <button
      v-if="!showInput"
      @click="handleResetApp"
      class="w-6 h-6 rounded-full bg-yellow-500 hover:bg-yellow-600 flex items-center justify-center transition-all shadow-md hover:shadow-lg active:scale-95 flex-shrink-0"
      title="重置程序"
    >
      <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd" />
      </svg>
    </button>
    <button
      v-if="!showInput"
      @click="handleExitApp"
      class="w-6 h-6 rounded-full bg-yellow-500 hover:bg-yellow-600 flex items-center justify-center transition-all shadow-md hover:shadow-lg active:scale-95 flex-shrink-0"
      title="退出程序"
    >
      <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
      </svg>
    </button>
    <button
      v-if="!showInput"
      @click="taskStore.showConfirm('删除已完成任务', '是否删除所有已完成任务？', () => taskStore.deleteCompletedTasks())"
      class="w-6 h-6 rounded-full bg-orange-500 hover:bg-orange-600 flex items-center justify-center transition-all shadow-md hover:shadow-lg active:scale-95 flex-shrink-0"
      title="删除已完成任务"
    >
      <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
      </svg>
    </button>
  </div>
</template>