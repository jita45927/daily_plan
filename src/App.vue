<script setup lang="ts">
import { onMounted, onUnmounted, nextTick } from 'vue'
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

  const target = e.target as HTMLElement
  
  // 检查点击是否在左键菜单内部
  const isInMainMenu = target.closest('.main-menu') !== null
  
  // 如果不在左键菜单内部，关闭左键菜单
  if (!isInMainMenu && taskStore.mainMenu.show) {
    taskStore.closeMainMenu()
  }
  
  // 关闭右键菜单
  invoke('close_context_menu').then(() => {
  }).catch(() => {
  })
  
  const noDragEl = target.closest('button, input, [contenteditable], .no-drag')
  if (noDragEl) {
    return
  }
  
  if (!taskStore.isWindowLocked) {
    invoke('start_dragging')
  }
}

const handleResizeStart = async (e: MouseEvent, direction: 'north' | 'south') => {
  e.preventDefault()
  e.stopPropagation()
  // 使用 clientY（逻辑像素坐标），与后端返回的逻辑像素坐标保持一致
  const startY = e.clientY

  // 立即通知后端开始调整大小，禁用自动收起
  await invoke('set_resizing', { resizing: true })

  const pos = await invoke('get_window_position') as [number, number, number, number]
  const startX = pos[0]
  const startYPos = pos[1]
  const startWidth = pos[2]
  const startHeight = pos[3]

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
    // 使用 clientY（逻辑像素坐标），与 startY 保持一致，无需额外缩放
    const delta = e.clientY - startY

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
    
    // 完成最终更新
    if (direction === 'north') {
      invoke('set_window_rect', { x: startX, y: targetY, width: startWidth, height: targetHeight })
        .then(() => {
          // 拖动上边缘改变窗口位置后，解除贴边状态
          invoke('reset_snap_state').catch(() => {})
          // 通知后端结束调整大小
          invoke('set_resizing', { resizing: false }).catch(() => {})
        })
    } else {
      invoke('set_window_size', { width: startWidth, height: targetHeight })
        .then(() => {
          // 通知后端结束调整大小
          invoke('set_resizing', { resizing: false }).catch(() => {})
        })
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
  await taskStore.loadTasks()
  
  // 使用 nextTick + requestAnimationFrame 确保 DOM 绘制完成后再通知后端
  await nextTick()
  requestAnimationFrame(() => {
    invoke('on_app_ready').catch(() => {})
  })

  await listen('timer_update', taskStore.handleTimerUpdate)
  await listen('timer_expired', taskStore.handleTimerExpired)
  await listen('app_focused', handleAppFocused)
  await listen('context_menu_command', handleContextMenuCommand)
  await listen('trash_context_menu_command', handleTrashContextMenuCommand)
  await listen('clean-computer-progress', taskStore.handleCleanComputerProgress)
  await listen('clean-computer-done', taskStore.handleCleanComputerDone)
  await listen('clean-duplicate-progress', taskStore.handleCleanDuplicateProgress)
  await listen('clean-duplicate-done', taskStore.handleCleanDuplicateDone)
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

    

    <!-- 清理电脑：非阻塞浮动徽章（不阻挡主窗口其他功能） -->
    <Teleport to="body">
      <Transition name="slide-fade">
        <div
          v-if="taskStore.isCleaningComputer"
          class="fixed bottom-3 right-3 z-40 bg-blue-500 text-white rounded-lg shadow-lg px-3 py-2 max-w-[260px] no-drag"
          style="pointer-events: auto;"
        >
          <div class="flex items-center gap-2 mb-1">
            <div class="w-3 h-3 border-2 border-white/40 border-t-white rounded-full animate-spin"></div>
            <span class="text-xs font-semibold">清理电脑中...</span>
          </div>
          <div class="text-[11px] text-white/90 leading-relaxed">
            <div>当前: {{ taskStore.cleanComputerStats?.currentCategory || '初始化' }}</div>
            <div>
              已删 {{ taskStore.cleanComputerStats?.deleted || 0 }} 个 ·
              跳过 {{ taskStore.cleanComputerStats?.skipped || 0 }} 个
            </div>
            <div>
              释放 {{ taskStore.formatBytes(taskStore.cleanComputerStats?.freedBytes || 0) }}
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- 清理重复文件：非阻塞浮动徽章（不阻挡主窗口其他功能） -->
    <Teleport to="body">
      <Transition name="slide-fade">
        <div
          v-if="taskStore.isCleaningDuplicates"
          class="fixed bottom-3 right-3 z-40 bg-orange-500 text-white rounded-lg shadow-lg px-3 py-2 max-w-[260px] no-drag"
          style="pointer-events: auto;"
        >
          <div class="flex items-center gap-2 mb-1">
            <div class="w-3 h-3 border-2 border-white/40 border-t-white rounded-full animate-spin"></div>
            <span class="text-xs font-semibold">清理重复文件中...</span>
          </div>
          <div class="text-[11px] text-white/90 leading-relaxed">
            <div>当前: {{ taskStore.cleanDuplicateStats?.currentCategory || '初始化' }}</div>
            <div>
              已移 {{ taskStore.cleanDuplicateStats?.moved || 0 }} 个 ·
              跳过 {{ taskStore.cleanDuplicateStats?.skipped || 0 }} 个
            </div>
            <div>
              共扫描 {{ taskStore.cleanDuplicateStats?.scanned || 0 }} 个文件
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- 清理电脑：完成通知（可关闭） -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="taskStore.cleanComputerNotice.show" class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
          <div class="bg-white rounded-lg p-6 shadow-xl max-w-sm mx-4">
            <h3 class="text-lg font-semibold text-gray-800 mb-2">{{ taskStore.cleanComputerNotice.title }}</h3>
            <p class="text-gray-600 mb-4 whitespace-pre-wrap text-sm">{{ taskStore.cleanComputerNotice.message }}</p>
            <div class="flex justify-end">
              <button
                @click="taskStore.hideCleanComputerNotice"
                class="px-4 py-2 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors"
              >
                确定
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- 清理重复文件：完成通知（可关闭） -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="taskStore.cleanDuplicateNotice.show" class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
          <div class="bg-white rounded-lg p-6 shadow-xl max-w-sm mx-4">
            <h3 class="text-lg font-semibold text-gray-800 mb-2">{{ taskStore.cleanDuplicateNotice.title }}</h3>
            <p class="text-gray-600 mb-4 whitespace-pre-wrap text-sm">{{ taskStore.cleanDuplicateNotice.message }}</p>
            <div class="flex justify-end">
              <button
                @click="taskStore.hideCleanDuplicateNotice"
                class="px-4 py-2 text-sm bg-orange-500 text-white rounded-lg hover:bg-orange-600 transition-colors"
              >
                确定
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <Teleport to="body">
      <Transition name="fade">
        <div v-if="taskStore.errorAlert.show" class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
          <div class="bg-white rounded-lg p-6 shadow-xl max-w-sm">
            <h3 class="text-lg font-semibold text-gray-800 mb-2">{{ taskStore.errorAlert.title }}</h3>
            <p class="text-gray-600 mb-4 whitespace-pre-wrap">{{ taskStore.errorAlert.message }}</p>
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

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
.slide-fade-enter-active {
  transition: all 0.25s ease;
}
.slide-fade-leave-active {
  transition: all 0.2s ease;
}
.slide-fade-enter-from,
.slide-fade-leave-to {
  opacity: 0;
  transform: translateY(10px);
}
</style>
