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

const handleResetApp = async () => {
  try {
    await invoke('reset_app_cmd')
    taskStore.tasks = []
    taskStore.timerStates.clear()
    taskStore.isWindowLocked = false
  } catch (error) {
    console.error('Failed to reset app:', error)
  }
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
      @click="handleResetApp"
      class="w-5 h-5 rounded-full bg-red-500 hover:bg-red-600 flex items-center justify-center transition-all shadow-md hover:shadow-lg active:scale-95 flex-shrink-0"
      title="重置程序（调试用）"
    >
      <svg xmlns="http://www.w3.org/2000/svg" class="h-2.5 w-2.5 text-white" viewBox="0 0 20 20" fill="currentColor">
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
  </div>
</template>