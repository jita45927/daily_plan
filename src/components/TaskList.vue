<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useTaskStore } from '../stores/taskStore'
import type { Task } from '../stores/taskStore'
import TaskItem from './TaskItem.vue'
import AddTaskBtn from './AddTaskBtn.vue'

const taskStore = useTaskStore()
const draggingTaskId = ref<number | null>(null)
const dragOverTaskId = ref<number | null>(null)
const dragStatus = ref<boolean | null>(null)
const dragStartY = ref(0)
const isDragging = ref(false)
const draggedTaskData = ref<Task | null>(null)
const dragMouseY = ref(0)
const dragMouseX = ref(0)

const handleMouseDown = (e: MouseEvent, task: Task) => {
  const target = e.target as HTMLElement
  if (target.closest('button, input, [contenteditable]')) {
    return
  }

  if (e.button === 2) {
    return
  }

  // 阻止冒泡到 App.vue，避免同时触发窗口拖拽
  e.stopPropagation()
  draggingTaskId.value = task.id
  dragStatus.value = task.status
  dragStartY.value = e.clientY
  draggedTaskData.value = task
  isDragging.value = false
  dragMouseY.value = e.clientY
  dragMouseX.value = e.clientX

  document.addEventListener('mousemove', handleMouseMove)
  document.addEventListener('mouseup', handleMouseUp)
}

const handleMouseMove = (e: MouseEvent) => {
  if (draggingTaskId.value === null) return

  const deltaY = Math.abs(e.clientY - dragStartY.value)

  if (!isDragging.value && deltaY > 5) {
    isDragging.value = true
  }

  if (isDragging.value) {
    dragMouseY.value = e.clientY
    dragMouseX.value = e.clientX

    const el = document.elementFromPoint(e.clientX, e.clientY)
    const taskEl = el?.closest('[data-task-id]') as HTMLElement
    if (taskEl) {
      const taskId = parseInt(taskEl.dataset.taskId || '0')
      if (taskId && taskId !== draggingTaskId.value) {
        dragOverTaskId.value = taskId
      }
    }
  }
}

const handleMouseUp = async () => {
  document.removeEventListener('mousemove', handleMouseMove)
  document.removeEventListener('mouseup', handleMouseUp)

  if (!isDragging.value || draggingTaskId.value === null || dragStatus.value === null) {
    resetDrag()
    return
  }

  const targetId = dragOverTaskId.value
  if (targetId === null || targetId === draggingTaskId.value) {
    resetDrag()
    return
  }

  const status = dragStatus.value
  const tasks = status ? [...taskStore.completedTasks] : [...taskStore.incompleteTasks]
  const fromIndex = tasks.findIndex(t => t.id === draggingTaskId.value)
  const toIndex = tasks.findIndex(t => t.id === targetId)

  if (fromIndex === -1 || toIndex === -1) {
    resetDrag()
    return
  }

  const [movedTask] = tasks.splice(fromIndex, 1)
  tasks.splice(toIndex, 0, movedTask)

  const taskIds = tasks.map(t => t.id)
  await taskStore.reorderTasks(taskIds, status)

  resetDrag()
}

const resetDrag = () => {
  draggingTaskId.value = null
  dragOverTaskId.value = null
  dragStatus.value = null
  isDragging.value = false
  draggedTaskData.value = null
}

onMounted(() => {
  taskStore.loadTasks()
})

onUnmounted(() => {
  document.removeEventListener('mousemove', handleMouseMove)
  document.removeEventListener('mouseup', handleMouseUp)
})
</script>

<template>
  <div class="flex-1 flex flex-col overflow-hidden">
    <div class="flex-1 overflow-y-auto py-1">
      <div v-if="taskStore.incompleteTasks.length === 0 && taskStore.completedTasks.length === 0" class="flex flex-col items-center justify-center h-full text-gray-600">
        <svg xmlns="http://www.w3.org/2000/svg" class="h-12 w-12 mb-3 opacity-40" viewBox="0 0 20 20" fill="currentColor">
          <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
        </svg>
        <p class="text-xs">暂无任务</p>
        <p class="text-[10px] mt-1 opacity-70">点击下方按钮添加</p>
      </div>

      <template v-else>
        <div v-if="taskStore.incompleteTasks.length > 0">
          <h3 class="text-[10px] font-medium text-gray-600/80 px-3 py-1.5 uppercase tracking-wider">待完成</h3>
          <div
            v-for="task in taskStore.incompleteTasks"
            :key="task.id"
            :data-task-id="task.id"
            @mousedown="(e) => handleMouseDown(e, task)"
          >
            <TaskItem
              :task="task"
              :is-dragging="draggingTaskId === task.id && isDragging"
              :is-drag-over="dragOverTaskId === task.id && dragStatus === false && isDragging"
            />
          </div>
        </div>

        <div v-if="taskStore.completedTasks.length > 0">
          <h3 class="text-[10px] font-medium text-gray-500/70 px-3 py-1.5 uppercase tracking-wider border-t border-yellow-600/20 mt-1">已完成</h3>
          <div
            v-for="task in taskStore.completedTasks"
            :key="task.id"
            :data-task-id="task.id"
            @mousedown="(e) => handleMouseDown(e, task)"
          >
            <TaskItem
              :task="task"
              :is-dragging="draggingTaskId === task.id && isDragging"
              :is-drag-over="dragOverTaskId === task.id && dragStatus === true && isDragging"
            />
          </div>
        </div>
      </template>
    </div>

    <AddTaskBtn />

    <!-- 拖拽时跟随鼠标的浮动元素 -->
    <Teleport to="body">
      <div
        v-if="isDragging && draggedTaskData"
        class="fixed pointer-events-none z-[9999] px-3 py-2 rounded-md shadow-lg text-sm leading-snug break-words max-w-[280px]"
        :style="{
          left: dragMouseX + 'px',
          top: dragMouseY + 'px',
          transform: 'translate(-50%, -50%)',
          backgroundColor: 'rgba(255, 218, 85, 0.95)',
          border: '1px solid rgba(180, 140, 0, 0.3)',
        }"
      >
        <div class="flex items-start gap-1.5">
          <div
            v-if="!draggedTaskData.status && !draggedTaskData.bold"
            class="w-2.5 h-2.5 rounded-full bg-gray-600/60 flex-shrink-0 mt-0.5"
          ></div>
          <span
            v-else-if="draggedTaskData.status"
            class="text-green-600 font-bold text-sm flex-shrink-0"
          >✓</span>
          <span
            v-else
            class="text-red-500 font-bold text-sm flex-shrink-0"
          >✕</span>
          <span
            class="flex-1"
            :style="{
              color: draggedTaskData.status ? '#6B7280' : (draggedTaskData.color || '#000000'),
              fontWeight: draggedTaskData.bold ? 'bold' : 'normal',
              textDecoration: draggedTaskData.status ? 'line-through' : 'none',
            }"
          >{{ draggedTaskData.text }}</span>
        </div>
      </div>
    </Teleport>
  </div>
</template>
