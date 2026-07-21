import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Task } from './types'
import { useAlertsStore } from './alerts'
import { useTimersStore } from './timers'

type TaskResponse = Task

export const useTasksStore = defineStore('tasks', () => {
  const tasks = ref<Task[]>([])
  const isWindowLocked = ref(false)
  const contextMenu = ref({
    show: false,
    x: 0,
    y: 0,
    taskId: 0
  })
  const mainMenu = ref({
    show: false,
    x: 0,
    y: 0
  })
  const isAnalyzingDesktop = ref(false)
  const isCleaningDuplicates = ref(false)

  const alertsStore = useAlertsStore()
  const timersStore = useTimersStore()

  const incompleteTasks = computed(() => tasks.value.filter(t => !t.status).sort((a, b) => a.orderIndex - b.orderIndex))
  const completedTasks = computed(() => tasks.value.filter(t => t.status).sort((a, b) => a.orderIndex - b.orderIndex))

  const validateCountdownMinutes = (minutes: string): { valid: boolean; message: string } => {
    const num = parseInt(minutes)
    if (isNaN(num)) {
      return { valid: false, message: '请输入有效的数字' }
    }
    if (num < 1 || num > 1440) {
      return { valid: false, message: '分钟数必须在 1-1440 之间' }
    }
    return { valid: true, message: '' }
  }

  const validateScheduledTime = (time: string): { valid: boolean; message: string } => {
    const regex = /^\d{4}\/\d{2}\/\d{2}-\d{2}:\d{2}$/
    if (!regex.test(time)) {
      return { valid: false, message: '时间格式必须为 YYYY/MM/DD-HH:MM' }
    }

    const [datePart, timePart] = time.split('-')
    const [year, month, day] = datePart.split('/').map(Number)
    const [hour, minute] = timePart.split(':').map(Number)

    const date = new Date(year, month - 1, day, hour, minute)
    const now = new Date()
    now.setSeconds(0, 0)

    if (isNaN(date.getTime())) {
      return { valid: false, message: '请输入有效的日期时间' }
    }

    if (date <= now) {
      return { valid: false, message: '目标时间不能早于当前时间' }
    }

    return { valid: true, message: '' }
  }

  const parseScheduledTime = (time: string): number | null => {
    const [datePart, timePart] = time.split('-')
    const [year, month, day] = datePart.split('/').map(Number)
    const [hour, minute] = timePart.split(':').map(Number)

    const date = new Date(year, month - 1, day, hour, minute)
    if (isNaN(date.getTime())) {
      return null
    }
    return Math.floor(date.getTime() / 1000)
  }

  const loadTasks = async () => {
    try {
      const result = await invoke<TaskResponse[]>('get_all_tasks_cmd')
      tasks.value = result
      await timersStore.restoreTimers(tasks.value)
    } catch (error) {
      await alertsStore.handleDbError(error, '加载任务')
    }
  }

  const addTask = async (text: string) => {
    try {
      const result = await invoke<TaskResponse>('insert_task_cmd', {
        text,
        status: false,
        color: '#000000',
        bold: false,
        timerType: '',
        timerValue: 0,
        timerRemaining: 0
      })
      tasks.value.push(result)
    } catch (error) {
      await alertsStore.handleDbError(error, '添加任务')
    }
  }

  const removeTask = async (id: number) => {
    try {
      await invoke('delete_task_cmd', { id })
      await timersStore.stopTimer(id)
      const index = tasks.value.findIndex(t => t.id === id)
      if (index !== -1) {
        tasks.value.splice(index, 1)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '删除任务')
    }
  }

  const markCompleted = async (id: number) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        await timersStore.stopTimer(id)
        const result = await invoke<TaskResponse>('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: true,
          color: '#000000',
          bold: false,
          timerType: task.timerType,
          timerValue: task.timerValue,
          timerRemaining: 0
        })
        Object.assign(task, result)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '标记完成')
    }
  }

  const markIncomplete = async (id: number) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        await timersStore.stopTimer(id)
        const result = await invoke<TaskResponse>('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: false,
          color: '#FF0000',
          bold: true,
          timerType: task.timerType,
          timerValue: task.timerValue,
          timerRemaining: task.timerRemaining
        })
        Object.assign(task, result)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '标记未完成')
    }
  }

  const resetTask = async (id: number) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        await timersStore.stopTimer(id)
        const result = await invoke<TaskResponse>('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: false,
          color: '#000000',
          bold: false,
          timerType: '',
          timerValue: 0,
          timerRemaining: 0
        })
        Object.assign(task, result)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '重置任务')
    }
  }

  const toggleWindowLock = async () => {
    try {
      const locked = await invoke<boolean>('toggle_window_lock')
      isWindowLocked.value = locked
    } catch (error) {
      console.error('Failed to toggle window lock:', error)
    }
  }

  const deleteCompletedTasks = async () => {
    try {
      for (const task of tasks.value) {
        if (task.status) {
          timersStore.timerStates.delete(task.id)
        }
      }
      await invoke('move_completed_to_trash_cmd')
      await loadTasks()
    } catch (error) {
      await alertsStore.handleDbError(error, '删除已完成任务')
    }
  }

  const reorderTasks = async (taskIds: number[], status: boolean) => {
    try {
      await invoke('reorder_tasks_cmd', { taskIds, status })
      const idToIndex = new Map<number, number>()
      taskIds.forEach((id, idx) => idToIndex.set(id, idx))
      for (const task of tasks.value) {
        if (task.status === status && idToIndex.has(task.id)) {
          task.orderIndex = idToIndex.get(task.id) || 0
        }
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '重新排序任务')
    }
  }

  const deleteAllTasks = async () => {
    try {
      timersStore.timerStates.clear()
      await invoke('move_all_to_trash_cmd')
      await loadTasks()
    } catch (error) {
      await alertsStore.handleDbError(error, '删除所有任务')
    }
  }

  const openContextMenu = (x: number, y: number, taskId: number) => {
    contextMenu.value = { show: true, x, y, taskId }
  }

  const closeContextMenu = () => {
    contextMenu.value = { show: false, x: 0, y: 0, taskId: 0 }
  }

  const openMainMenu = (x: number, y: number) => {
    mainMenu.value = { show: true, x, y }
    invoke('set_main_menu_open', { open: true }).catch(() => {})
  }

  const closeMainMenu = () => {
    mainMenu.value = { show: false, x: 0, y: 0 }
    invoke('set_main_menu_open', { open: false }).catch(() => {})
  }

  const updateTaskText = async (id: number, text: string) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        const result = await invoke<TaskResponse>('update_task_cmd', {
          id: task.id,
          text,
          status: task.status,
          color: task.color,
          bold: task.bold,
          timerType: task.timerType,
          timerValue: task.timerValue,
          timerRemaining: task.timerRemaining
        })
        Object.assign(task, result)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '更新任务文本')
    }
  }

  const updateTaskColor = async (id: number, color: string) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task && !task.status) {
        const result = await invoke<TaskResponse>('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: task.status,
          color,
          bold: task.bold,
          timerType: task.timerType,
          timerValue: task.timerValue,
          timerRemaining: task.timerRemaining
        })
        Object.assign(task, result)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '更新任务颜色')
    }
  }

  const updateTaskBold = async (id: number, bold: boolean) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        const result = await invoke<TaskResponse>('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: task.status,
          color: task.color,
          bold,
          timerType: task.timerType,
          timerValue: task.timerValue,
          timerRemaining: task.timerRemaining
        })
        Object.assign(task, result)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '更新任务粗体')
    }
  }

  const resetTaskStyle = async (id: number) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        const newColor = '#000000'
        const newBold = task.status ? false : true
        const result = await invoke<TaskResponse>('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: task.status,
          color: newColor,
          bold: newBold,
          timerType: task.timerType,
          timerValue: task.timerValue,
          timerRemaining: task.timerRemaining
        })
        Object.assign(task, result)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '重置任务样式')
    }
  }

  const setTaskTimer = async (id: number, timerType: string, timerValue: number) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        const result = await invoke<TaskResponse>('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: task.status,
          color: task.color,
          bold: task.bold,
          timerType: timerType,
          timerValue: timerValue,
          timerRemaining: timerType ? timerValue : 0
        })
        Object.assign(task, result)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '设置任务定时器')
    }
  }

  return {
    tasks,
    isWindowLocked,
    contextMenu,
    mainMenu,
    isAnalyzingDesktop,
    isCleaningDuplicates,
    incompleteTasks,
    completedTasks,
    loadTasks,
    addTask,
    removeTask,
    markCompleted,
    markIncomplete,
    resetTask,
    toggleWindowLock,
    deleteCompletedTasks,
    deleteAllTasks,
    reorderTasks,
    openContextMenu,
    closeContextMenu,
    openMainMenu,
    closeMainMenu,
    updateTaskText,
    updateTaskColor,
    updateTaskBold,
    resetTaskStyle,
    setTaskTimer,
    validateCountdownMinutes,
    validateScheduledTime,
    parseScheduledTime
  }
})