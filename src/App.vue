<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { useTaskStore } from './stores/taskStore'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import WindowHeader from './components/WindowHeader.vue'
import TaskList from './components/TaskList.vue'
import ConfirmDialog from './components/Popups/ConfirmDialog.vue'
import CountdownAlert from './components/Popups/CountdownAlert.vue'
import TrashWindow from './components/TrashWindow.vue'
import MainMenu from './components/MainMenu.vue'

const taskStore = useTaskStore()

const handleMouseDown = (e: MouseEvent) => {
  if (e.button === 2) {
    return
  }

  invoke('close_context_menu').then(() => {
  }).catch(() => {
  })
  
  const target = e.target as HTMLElement
  const noDragEl = target.closest('button, input, [contenteditable], .no-drag')
  if (noDragEl) {
    return
  }

  if (taskStore.mainMenu.show) {
    taskStore.closeMainMenu()
    return
  }
  
  if (!taskStore.isWindowLocked) {
    invoke('start_dragging')
  }
}

const handleResizeStart = async (e: MouseEvent, direction: 'north' | 'south') => {
  e.preventDefault()
  e.stopPropagation()
  const startY = e.screenY

  const pos = await invoke('get_window_position') as [number, number, number, number]
  const startX = pos[0]
  const startYPos = pos[1]
  const startWidth = pos[2]
  const startHeight = pos[3]

  const devicePixelRatio = window.devicePixelRatio || 1

  let rafId: number | null = null
  let isInvoking = false
  let needsUpdate = false
  let targetHeight = startHeight
  let targetY = startYPos

  const sendUpdate = () => {
    isInvoking = true
    needsUpdate = false
    const promise = direction === 'north'
      ? invoke('set_window_rect', { x: startX, y: targetY, width: startWidth, height: targetHeight })
      : invoke('set_window_size', { width: startWidth, height: targetHeight })
    promise.then(() => {
      isInvoking = false
      if (needsUpdate) {
        sendUpdate()
      }
    })
  }

  const handleMouseMove = (e: MouseEvent) => {
    const delta = (e.screenY - startY) / devicePixelRatio

    if (direction === 'south') {
      targetHeight = Math.max(300, Math.min(startHeight + delta, 9999))
    } else {
      targetHeight = Math.max(300, Math.min(startHeight - delta, 9999))
      targetY = startYPos + startHeight - targetHeight
    }

    if (isInvoking) {
      needsUpdate = true
    } else if (rafId === null) {
      rafId = requestAnimationFrame(() => {
        rafId = null
        sendUpdate()
      })
    }
  }

  const handleMouseUp = () => {
    document.removeEventListener('mousemove', handleMouseMove)
    document.removeEventListener('mouseup', handleMouseUp)
    if (rafId !== null) {
      cancelAnimationFrame(rafId)
      rafId = null
    }
    if (direction === 'north') {
      invoke('set_window_rect', { x: startX, y: targetY, width: startWidth, height: targetHeight })
    } else {
      invoke('set_window_size', { width: startWidth, height: targetHeight })
    }
  }

  document.addEventListener('mousemove', handleMouseMove)
  document.addEventListener('mouseup', handleMouseUp)
}

const handleMouseUp = () => {
  invoke('stop_dragging')
}

const handleAppFocused = async () => {
  await taskStore.calibrateAllTimers()
}

interface ContextMenuCommand {
  action: string
  taskId: number
  value?: string
}

const handleContextMenuCommand = async (event: { payload: ContextMenuCommand }) => {
  const { action, taskId, value } = event.payload

  switch (action) {
    case 'mark_completed':
      taskStore.markCompleted(taskId)
      break
    case 'mark_incomplete':
      taskStore.markIncomplete(taskId)
      break
    case 'delete':
      taskStore.showConfirm('删除任务', '确定要删除这个任务吗？', () => {
        taskStore.moveToTrash(taskId)
      })
      break
    case 'restore':
      taskStore.resetTask(taskId)
      break
    case 'start_countdown':
      if (value) {
        taskStore.startCountdown(taskId, parseInt(value))
      }
      break
    case 'start_scheduled':
      if (value) {
        taskStore.startScheduledTimer(taskId, parseInt(value))
      }
      break
    case 'stop_timer':
      taskStore.stopTimer(taskId)
      break
    case 'toggle_bold':
      const task = taskStore.tasks.find(t => t.id === taskId)
      if (task) {
        taskStore.updateTaskBold(taskId, !task.bold)
      }
      break
    case 'set_color':
      if (value) {
        taskStore.updateTaskColor(taskId, value)
      }
      break
    case 'reset_style':
      taskStore.resetTaskStyle(taskId)
      break
    case 'clear_completed':
      taskStore.showConfirm('删除已完成任务', '是否删除所有已完成任务？', () => {
        taskStore.deleteCompletedTasks()
      })
      break
    case 'clear_all':
      taskStore.showConfirm('删除所有任务', '是否删除所有任务？', () => {
        taskStore.deleteAllTasks()
      })
      break
  }
}

interface TrashContextMenuCommand {
  action: string
  taskId: number
}

const handleTrashContextMenuCommand = async (event: { payload: TrashContextMenuCommand }) => {
  const { action, taskId } = event.payload

  switch (action) {
    case 'permanently_delete':
      taskStore.showConfirm('彻底删除', '确定要彻底删除这个任务吗？此操作不可恢复。', () => {
        taskStore.permanentlyDelete(taskId)
      })
      break
    case 'restore':
      taskStore.restoreFromTrash(taskId)
      break
    case 'clear_one_week':
      taskStore.showConfirm('清理回收站', '确定要删除一周前的所有回收站任务吗？', () => {
        taskStore.clearTrashByPeriod(7)
      })
      break
    case 'clear_two_weeks':
      taskStore.showConfirm('清理回收站', '确定要删除两周前的所有回收站任务吗？', () => {
        taskStore.clearTrashByPeriod(14)
      })
      break
    case 'clear_month':
      taskStore.showConfirm('清理回收站', '确定要删除一个月前的所有回收站任务吗？', () => {
        taskStore.clearTrashByPeriod(30)
      })
      break
    case 'clear_all':
      taskStore.showConfirm('清理回收站', '确定要清空回收站吗？所有任务将被彻底删除！', () => {
        taskStore.clearTrashByPeriod(0)
      })
      break
  }
}

onMounted(async () => {
  taskStore.loadTasks()

  await listen('timer_update', taskStore.handleTimerUpdate)
  await listen('timer_expired', taskStore.handleTimerExpired)
  await listen('app_focused', handleAppFocused)
  await listen('context_menu_command', handleContextMenuCommand)
  await listen('trash_context_menu_command', handleTrashContextMenuCommand)
  await listen('window_collapsed', () => {
    taskStore.closeMainMenu()
  })
  
  document.addEventListener('mouseup', handleMouseUp)
})

onUnmounted(() => {
  document.removeEventListener('mouseup', handleMouseUp)
})
</script>

<template>
  <div class="w-full h-full flex flex-col rounded-lg overflow-hidden" :style="{ backgroundColor: taskStore.isWindowLocked ? '#F5C820' : '#FFD028' }">
    <!-- 顶部调整手柄 -->
    <div 
      @mousedown="(e) => handleResizeStart(e, 'north')"
      class="h-2 bg-yellow-600/30 cursor-ns-resize hover:bg-yellow-600/50 transition-colors flex-shrink-0"
    ></div>
    
    <div @mousedown="handleMouseDown" class="flex-1 flex flex-col cursor-move overflow-hidden">
      <WindowHeader />
      
      <div class="border-t border-yellow-600/20"></div>
      
      <TaskList />
    </div>
    
    <!-- 底部调整手柄 -->
    <div 
      @mousedown="(e) => handleResizeStart(e, 'south')"
      class="h-2 bg-yellow-600/30 cursor-ns-resize hover:bg-yellow-600/50 transition-colors flex-shrink-0"
    ></div>
    
    <ConfirmDialog />
    <CountdownAlert />
    <TrashWindow />
    <MainMenu />
    
    <!-- 桌面分析中 loading 遮罩 -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="taskStore.isAnalyzingDesktop" class="fixed inset-0 bg-yellow-400/90 flex flex-col items-center justify-center z-50">
          <div class="w-12 h-12 border-4 border-white/30 border-t-white rounded-full animate-spin mb-4"></div>
          <p class="text-white text-base font-medium">桌面分析中，请稍后...</p>
        </div>
      </Transition>
    </Teleport>
    
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="taskStore.errorAlert.show" class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
          <div class="bg-white rounded-lg p-6 shadow-xl max-w-sm">
            <h3 class="text-lg font-semibold text-gray-800 mb-2">{{ taskStore.errorAlert.title }}</h3>
            <p class="text-gray-600 mb-4">{{ taskStore.errorAlert.message }}</p>
            <div class="flex justify-end">
              <button
                @click="taskStore.hideErrorAlert"
                class="px-4 py-2 text-sm bg-yellow-500 text-white rounded-lg hover:bg-yellow-600 transition-colors"
              >
                确定
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
