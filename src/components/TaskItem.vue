<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTaskStore, type Task } from '../stores/taskStore'
import TimeInput from './Popups/TimeInput.vue'

const props = defineProps<{
  task: Task
  isDragging?: boolean
  isDragOver?: boolean
}>()

const taskStore = useTaskStore()
const isHovered = ref(false)
const timeInputMode = ref<'countdown' | 'scheduled' | null>(null)
const isEditing = ref(false)
const editText = ref('')
const editInputRef = ref<HTMLInputElement | null>(null)

const timerState = computed(() => {
  return taskStore.timerStates.get(props.task.id)
})

const timerStyle = computed(() => {
  if (!timerState.value) return ''
  if (timerState.value.remaining <= 60) return 'text-red-500'
  if (timerState.value.remaining <= 300) return 'text-orange-500'
  return 'text-blue-500'
})

const textStyle = computed(() => {
  const styles: Record<string, string> = {}
  if (props.task.status) {
    styles.color = '#6B7280'
    styles.fontWeight = 'normal'
  } else {
    styles.color = props.task.color || '#000000'
    styles.fontWeight = props.task.bold ? 'bold' : 'normal'
  }
  return styles
})

const handleMarkCompleted = () => {
  taskStore.markCompleted(props.task.id)
}

const handleMarkIncomplete = () => {
  taskStore.markIncomplete(props.task.id)
}

const handleDelete = () => {
  taskStore.showConfirm('删除任务', '确定要删除这个任务吗？', () => {
    taskStore.moveToTrash(props.task.id)
  })
}

const handleReset = () => {
  taskStore.resetTask(props.task.id)
}

const handleContextMenu = (e: MouseEvent) => {
  e.preventDefault()
  e.stopPropagation()
  
  // 关闭左键菜单，确保不会同时显示两个菜单
  taskStore.closeMainMenu()
  
  invoke('show_context_menu', {
    screenX: e.screenX,
    screenY: e.screenY,
    task: {
      id: props.task.id,
      text: props.task.text,
      status: props.task.status,
      bold: props.task.bold,
      color: props.task.color,
      timerType: props.task.timerType,
      timerRemaining: props.task.timerRemaining,
    }
  }).then(() => {
  }).catch(() => {
  })
}

const handleStartCountdown = () => {
  timeInputMode.value = 'countdown'
  taskStore.openPopup('timeInput')
}

const handleStartScheduled = () => {
  timeInputMode.value = 'scheduled'
  taskStore.openPopup('timeInput')
}

const handleTimeInputSelect = (value: number) => {
  if (timeInputMode.value === 'countdown') {
    taskStore.startCountdown(props.task.id, value)
  } else if (timeInputMode.value === 'scheduled') {
    taskStore.startScheduledTimer(props.task.id, value)
  }
  timeInputMode.value = null
}

const handleStopTimer = () => {
  taskStore.stopTimer(props.task.id)
}

const handleCalibrate = () => {
  taskStore.calibrateTimer(props.task.id)
}

const handleDoubleClick = () => {
  if (props.task.status) return
  isEditing.value = true
  editText.value = props.task.text
  nextTick(() => {
    editInputRef.value?.focus()
    editInputRef.value?.select()
  })
}

const handleEditSave = () => {
  const trimmed = editText.value.trim()
  if (trimmed && trimmed !== props.task.text) {
    taskStore.updateTaskText(props.task.id, trimmed)
  }
  isEditing.value = false
}

const handleEditCancel = () => {
  isEditing.value = false
}

const handleEditKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Enter') {
    handleEditSave()
  } else if (e.key === 'Escape') {
    handleEditCancel()
  }
}

const handleEditBlur = () => {
  if (isEditing.value) {
    handleEditSave()
  }
}

onMounted(() => {
  if (props.task.timerType && props.task.timerRemaining > 0) {
    taskStore.getTimerStatus(props.task.id)
  }
})

onUnmounted(() => {
})
</script>

<template>
  <div
    class="flex flex-col gap-1.5 px-3 py-2 border-b border-yellow-600/20 transition-all duration-150 select-none cursor-grab active:cursor-grabbing"
    :style="{ backgroundColor: isHovered ? '#FFE890' : '#FFDA55' }"
    :class="[
      isDragging ? 'opacity-30' : '',
      isDragOver ? 'border-t-2 border-t-red-500' : ''
    ]"
    @mouseenter="isHovered = true"
    @mouseleave="isHovered = false"
    @contextmenu="handleContextMenu"
  >
    <div class="flex items-start gap-2">
      <div class="flex-shrink-0 w-5 h-5 flex items-center justify-center pt-0.5">
        <div
          v-if="!task.status && task.color !== '#FF0000'"
          class="w-2.5 h-2.5 rounded-full bg-gray-600/60"
        ></div>
        <span
          v-else-if="task.status"
          class="text-green-600 font-bold text-sm"
        >✓</span>
        <span
          v-else-if="!task.status && task.color === '#FF0000'"
          class="text-red-500 font-bold text-sm"
        >✕</span>
      </div>
      
      <div class="flex-1 min-w-0">
        <div class="flex items-start gap-1.5">
          <template v-if="isEditing">
            <input
              :id="`edit-task-${task.id}`"
              :name="`edit-task-${task.id}`"
              ref="editInputRef"
              v-model="editText"
              class="flex-1 text-xs leading-snug bg-white/90 rounded px-1.5 py-0.5 focus:outline-none focus:ring-1 focus:ring-yellow-500 text-gray-800"
              :style="{ color: task.color }"
              @keydown="handleEditKeydown"
              @blur="handleEditBlur"
              autocomplete="off"
            />
          </template>
          <p
            v-else
            class="text-sm leading-snug break-words flex-1 cursor-text select-none"
            :class="[task.status ? 'line-through' : '']"
            :style="textStyle"
            @dblclick="handleDoubleClick"
          >
            {{ task.text }}
          </p>
        </div>
        
        <div
          v-if="timerState"
          class="mt-0.5 text-xs font-mono font-semibold"
          :class="timerStyle"
        >
          {{ timerState.formatted }}
          <span class="text-[10px] ml-1 opacity-60">
            {{ timerState.is_running ? '⏳' : '⏹' }}
          </span>
        </div>
        
        <div
          v-else-if="task.timerType && task.timerRemaining === 0"
          class="mt-0.5 text-xs text-gray-500"
        >
          {{ task.timerType === 'countdown' ? '倒计时已结束' : '定时任务已完成' }}
        </div>
      </div>
    </div>
    
    <div
      class="flex items-center gap-1 flex-wrap transition-opacity duration-150 pl-7"
      :class="[
        isHovered ? 'opacity-100 h-auto' : 'opacity-0 h-0 overflow-hidden'
      ]"
    >
      <button
        v-if="!timerState"
        @click="handleStartCountdown"
        class="w-6 h-6 rounded-full bg-blue-500 hover:bg-blue-600 flex items-center justify-center transition-colors shadow-sm flex-shrink-0"
        title="开始倒计时"
      >
        <span class="text-white text-[10px] font-bold">⏱</span>
      </button>
      
      <button
        v-if="!timerState"
        @click="handleStartScheduled"
        class="w-6 h-6 rounded-full bg-purple-500 hover:bg-purple-600 flex items-center justify-center transition-colors shadow-sm flex-shrink-0"
        title="设置定时"
      >
        <span class="text-white text-[10px] font-bold">⏰</span>
      </button>
      
      <button
        v-if="timerState"
        @click="handleStopTimer"
        class="w-6 h-6 rounded-full bg-gray-500 hover:bg-gray-600 flex items-center justify-center transition-colors shadow-sm flex-shrink-0"
        title="停止计时"
      >
        <span class="text-white text-[10px] font-bold">⏹</span>
      </button>
      
      <button
        v-if="timerState"
        @click="handleCalibrate"
        class="w-6 h-6 rounded-full bg-cyan-500 hover:bg-cyan-600 flex items-center justify-center transition-colors shadow-sm flex-shrink-0"
        title="校准时间"
      >
        <span class="text-white text-[10px] font-bold">🔄</span>
      </button>
      
      <button
        @click="handleMarkCompleted"
        class="w-6 h-6 rounded-full bg-green-500 hover:bg-green-600 flex items-center justify-center transition-colors shadow-sm flex-shrink-0"
        title="标记已完成"
      >
        <span class="text-white text-[10px] font-bold">✓</span>
      </button>
      
      <button
        @click="handleMarkIncomplete"
        class="w-6 h-6 rounded-full bg-red-500 hover:bg-red-600 flex items-center justify-center transition-colors shadow-sm flex-shrink-0"
        title="标记未完成"
      >
        <span class="text-white text-[10px] font-bold">✕</span>
      </button>
      
      <button
        @click="handleDelete"
        class="w-6 h-6 rounded-full bg-yellow-500 hover:bg-yellow-600 flex items-center justify-center transition-colors shadow-sm flex-shrink-0"
        title="删除任务"
      >
        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
          <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
        </svg>
      </button>
      
      <button
        @click="handleReset"
        class="w-6 h-6 rounded-full bg-gray-500 hover:bg-gray-600 flex items-center justify-center transition-colors shadow-sm flex-shrink-0"
        title="恢复任务"
      >
        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 text-white" viewBox="0 0 20 20" fill="currentColor">
          <path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd" />
        </svg>
      </button>
    </div>
  </div>
  
  <TimeInput
    v-if="timeInputMode"
    :mode="timeInputMode"
    @select="handleTimeInputSelect"
  />
</template>