import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Task {
  id: number
  text: string
  status: boolean
  color: string
  bold: boolean
  timerType: string
  timerValue: number
  timerRemaining: number
  created_at: string
  orderIndex: number
}

export interface DeletedTask {
  id: number
  originalId: number
  text: string
  status: boolean
  color: string
  bold: boolean
  timerType: string
  timerValue: number
  timerRemaining: number
  created_at: string
  orderIndex: number
  deletedAt: string
}

type TaskResponse = Task
type DeletedTaskResponse = DeletedTask

export interface TimerState {
  task_id: number
  remaining: number
  hours: number
  minutes: number
  seconds: number
  formatted: string
  is_running: boolean
}

export interface ExpiredTask {
  task_id: number
  task_title: string
  timerType: string
  lastTimerValue: number
}

export interface ErrorAlert {
  show: boolean
  title: string
  message: string
}

export const useTaskStore = defineStore('tasks', () => {
  const tasks = ref<Task[]>([])
  const isWindowLocked = ref(false)
  const confirmDialog = ref({
    show: false,
    title: '',
    message: '',
    onConfirm: () => {}
  })

  const errorAlert = ref<ErrorAlert>({
    show: false,
    title: '',
    message: ''
  })

  const activePopups = ref<{
    colorPicker?: boolean
    timeInput?: boolean
    countdownAlert?: boolean
  }>({})

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

  const timerStates = ref<Map<number, TimerState>>(new Map())
  const expiredTask = ref<ExpiredTask | null>(null)
  const deletedTasks = ref<DeletedTask[]>([])
  const trashWindowVisible = ref(false)

  const incompleteTasks = computed(() => tasks.value.filter(t => !t.status).sort((a, b) => a.orderIndex - b.orderIndex))
  const completedTasks = computed(() => tasks.value.filter(t => t.status).sort((a, b) => a.orderIndex - b.orderIndex))

  const showErrorAlert = (title: string, message: string) => {
    errorAlert.value = { show: true, title, message }
  }

  const hideErrorAlert = () => {
    errorAlert.value = { show: false, title: '', message: '' }
  }

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

  const handleDbError = async (error: unknown, action: string) => {
    console.error(`Database error during ${action}:`, error)
    const errorMsg = error instanceof Error ? error.message : String(error)
    
    if (errorMsg.includes('损坏') || errorMsg.includes('corrupted') || errorMsg.includes('CannotOpen')) {
      showErrorAlert('数据库损坏', '检测到数据库文件损坏，已自动重置为空白任务列表。')
      await invoke('reinitialize_db_cmd')
      tasks.value = []
      timerStates.value.clear()
    } else {
      showErrorAlert('数据库错误', `执行${action}时发生错误: ${errorMsg}`)
    }
  }

  const loadTasks = async () => {
    try {
      const result = await invoke<TaskResponse[]>('get_all_tasks_cmd')
      tasks.value = result
    } catch (error) {
      await handleDbError(error, '加载任务')
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
      await handleDbError(error, '添加任务')
    }
  }

  const removeTask = async (id: number) => {
    try {
      await invoke('delete_task_cmd', { id })
      await invoke('stop_timer_cmd', { taskId: id })
      timerStates.value.delete(id)
      const index = tasks.value.findIndex(t => t.id === id)
      if (index !== -1) {
        tasks.value.splice(index, 1)
      }
    } catch (error) {
      await handleDbError(error, '删除任务')
    }
  }

  const markCompleted = async (id: number) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        await invoke('stop_timer_cmd', { taskId: id })
        timerStates.value.delete(id)
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
      await handleDbError(error, '标记完成')
    }
  }

  const markIncomplete = async (id: number) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
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
      await handleDbError(error, '标记未完成')
    }
  }

  const resetTask = async (id: number) => {
    try {
      const task = tasks.value.find(t => t.id === id)
      if (task) {
        await invoke('stop_timer_cmd', { taskId: id })
        timerStates.value.delete(id)
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
      await handleDbError(error, '重置任务')
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

  const showConfirm = (title: string, message: string, onConfirm: () => void) => {
    confirmDialog.value = { show: true, title, message, onConfirm }
  }

  const hideConfirm = () => {
    confirmDialog.value = { show: false, title: '', message: '', onConfirm: () => {} }
  }

  const openPopup = (popup: keyof typeof activePopups.value) => {
    activePopups.value[popup] = true
  }

  const closePopup = (popup: keyof typeof activePopups.value) => {
    activePopups.value[popup] = false
  }

  const deleteCompletedTasks = async () => {
    try {
      for (const task of tasks.value) {
        if (task.status) {
          timerStates.value.delete(task.id)
        }
      }
      await invoke('move_completed_to_trash_cmd')
      await loadTasks()
    } catch (error) {
      await handleDbError(error, '删除已完成任务')
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
      await handleDbError(error, '重新排序任务')
    }
  }

  const deleteAllTasks = async () => {
    try {
      timerStates.value.clear()
      await invoke('move_all_to_trash_cmd')
      await loadTasks()
    } catch (error) {
      await handleDbError(error, '删除所有任务')
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
  }

  const closeMainMenu = () => {
    mainMenu.value = { show: false, x: 0, y: 0 }
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
      await handleDbError(error, '更新任务文本')
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
      await handleDbError(error, '更新任务颜色')
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
      await handleDbError(error, '更新任务粗体')
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
      await handleDbError(error, '重置任务样式')
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
      await handleDbError(error, '设置任务定时器')
    }
  }

  const startCountdown = async (taskId: number, minutes: number) => {
    const validation = validateCountdownMinutes(minutes.toString())
    if (!validation.valid) {
      showErrorAlert('输入错误', validation.message)
      return
    }

    try {
      await invoke('start_countdown_cmd', { taskId, minutes })
      const task = tasks.value.find(t => t.id === taskId)
      if (task) {
        task.timerType = 'countdown'
        task.timerValue = minutes * 60
        task.timerRemaining = minutes * 60
        task.status = false
        task.color = '#000000'
        task.bold = false
        await invoke('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: false,
          color: '#000000',
          bold: false,
          timerType: 'countdown',
          timerValue: minutes * 60,
          timerRemaining: minutes * 60
        })
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      showErrorAlert('启动倒计时失败', errorMsg)
    }
  }

  const startScheduledTimer = async (taskId: number, targetTimestamp: number) => {
    try {
      const result = await invoke('start_scheduled_timer_cmd', { taskId, targetTimestamp: targetTimestamp })
      if (result === '目标时间必须大于当前时间') {
        showErrorAlert('输入错误', '目标时间不能早于当前时间')
        return
      }
      
      const task = tasks.value.find(t => t.id === taskId)
      if (task) {
        task.timerType = 'scheduled'
        task.timerValue = targetTimestamp
        const currentTime = Math.floor(Date.now() / 1000)
        task.timerRemaining = targetTimestamp - currentTime
        task.status = false
        task.color = '#000000'
        task.bold = false
        await invoke('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: false,
          color: '#000000',
          bold: false,
          timerType: 'scheduled',
          timerValue: targetTimestamp,
          timerRemaining: targetTimestamp - currentTime
        })
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      showErrorAlert('启动定时任务失败', errorMsg)
    }
  }

  const stopTimer = async (taskId: number) => {
    try {
      await invoke('stop_timer_cmd', { taskId: taskId })
      timerStates.value.delete(taskId)
      const task = tasks.value.find(t => t.id === taskId)
      if (task) {
        await invoke('update_task_cmd', {
          id: task.id,
          text: task.text,
          status: task.status,
          color: task.color,
          bold: task.bold,
          timerType: task.timerType,
          timerValue: task.timerValue,
          timerRemaining: task.timerRemaining
        })
      }
    } catch (error) {
      console.error('Failed to stop timer:', error)
    }
  }

  const getTimerStatus = async (taskId: number) => {
    try {
      const status = await invoke<TimerState | null>('get_timer_status_cmd', { taskId })
      if (status) {
        timerStates.value.set(taskId, status)
      }
      return status
    } catch (error) {
      console.error('Failed to get timer status:', error)
      return null
    }
  }

  const calibrateTimer = async (taskId: number) => {
    try {
      const status = await invoke<TimerState | null>('calibrate_timer_cmd', { taskId })
      if (status) {
        timerStates.value.set(taskId, status)
        const task = tasks.value.find(t => t.id === taskId)
        if (task) {
          task.timerRemaining = status.remaining
          await invoke('update_task_cmd', {
            id: task.id,
            text: task.text,
            status: task.status,
            color: task.color,
            bold: task.bold,
            timerType: task.timerType,
            timerValue: task.timerValue,
            timerRemaining: status.remaining
          })
        }
      }
      return status
    } catch (error) {
      console.error('Failed to calibrate timer:', error)
      return null
    }
  }

  const calibrateAllTimers = async () => {
    for (const task of tasks.value) {
      if (task.timerType && task.timerRemaining > 0) {
        await calibrateTimer(task.id)
      }
    }
  }

  const handleTimerUpdate = async (event: { payload: { task_id: number; remaining: number; hours: number; minutes: number; seconds: number; formatted: string } }) => {
    const { task_id, remaining, hours, minutes, seconds, formatted } = event.payload
    timerStates.value.set(task_id, {
      task_id,
      remaining,
      hours,
      minutes,
      seconds,
      formatted,
      is_running: true
    })

    const task = tasks.value.find(t => t.id === task_id)
    if (task) {
      task.timerRemaining = remaining
      await invoke('update_task_cmd', {
        id: task.id,
        text: task.text,
        status: task.status,
        color: task.color,
        bold: task.bold,
        timerType: task.timerType,
        timerValue: task.timerValue,
        timerRemaining: remaining
      })
    }
  }

  const handleTimerExpired = async (event: { payload: { task_id: number; timerType: string } }) => {
    const { task_id, timerType } = event.payload
    timerStates.value.delete(task_id)

    const task = tasks.value.find(t => t.id === task_id)
    if (task) {
      task.timerRemaining = 0
      await invoke('update_task_cmd', {
        id: task.id,
        text: task.text,
        status: task.status,
        color: task.color,
        bold: task.bold,
        timerType: task.timerType,
        timerValue: task.timerValue,
        timerRemaining: 0
      })

      expiredTask.value = {
        task_id,
        task_title: task.text,
        timerType,
        lastTimerValue: task.timerValue
      }
      openPopup('countdownAlert')
    }
  }

  const resetExpiredTask = () => {
    expiredTask.value = null
    closePopup('countdownAlert')
  }

  const moveToTrash = async (id: number) => {
    try {
      await invoke('stop_timer_cmd', { taskId: id })
      timerStates.value.delete(id)
      await invoke('move_task_to_trash_cmd', { taskId: id })
      const index = tasks.value.findIndex(t => t.id === id)
      if (index !== -1) {
        tasks.value.splice(index, 1)
      }
    } catch (error) {
      await handleDbError(error, '移到回收站')
    }
  }

  const loadDeletedTasks = async () => {
    try {
      const result = await invoke<DeletedTaskResponse[]>('get_deleted_tasks_cmd')
      deletedTasks.value = result
    } catch (error) {
      await handleDbError(error, '加载回收站')
    }
  }

  const restoreFromTrash = async (deletedId: number) => {
    try {
      await invoke('restore_task_cmd', { deletedId })
      const index = deletedTasks.value.findIndex(t => t.id === deletedId)
      if (index !== -1) {
        deletedTasks.value.splice(index, 1)
      }
      await loadTasks()
    } catch (error) {
      await handleDbError(error, '恢复任务')
    }
  }

  const permanentlyDelete = async (deletedId: number) => {
    try {
      await invoke('permanently_delete_task_cmd', { deletedId })
      const index = deletedTasks.value.findIndex(t => t.id === deletedId)
      if (index !== -1) {
        deletedTasks.value.splice(index, 1)
      }
    } catch (error) {
      await handleDbError(error, '彻底删除任务')
    }
  }

  const clearTrashByPeriod = async (periodDays: number) => {
    try {
      await invoke('clear_trash_by_period_cmd', { periodDays })
      await loadDeletedTasks()
    } catch (error) {
      await handleDbError(error, '清理回收站')
    }
  }

  const openTrashWindow = () => {
    trashWindowVisible.value = true
    loadDeletedTasks()
  }

  const closeTrashWindow = () => {
    trashWindowVisible.value = false
  }

  const reStartTimer = async () => {
    if (!expiredTask.value) return

    const { task_id, timerType, lastTimerValue } = expiredTask.value
    if (timerType === 'countdown') {
      const minutes = Math.floor(lastTimerValue / 60)
      await startCountdown(task_id, minutes)
    } else if (timerType === 'scheduled') {
      const currentTime = Math.floor(Date.now() / 1000)
      const newTarget = currentTime + lastTimerValue
      await startScheduledTimer(task_id, newTarget)
    }
    resetExpiredTask()
  }

  return {
    tasks,
    isWindowLocked,
    confirmDialog,
    errorAlert,
    activePopups,
    contextMenu,
    mainMenu,
    isAnalyzingDesktop,
    timerStates,
    expiredTask,
    deletedTasks,
    trashWindowVisible,
    incompleteTasks,
    completedTasks,
    loadTasks,
    addTask,
    removeTask,
    markCompleted,
    markIncomplete,
    resetTask,
    toggleWindowLock,
    showConfirm,
    hideConfirm,
    showErrorAlert,
    hideErrorAlert,
    openPopup,
    closePopup,
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
    startCountdown,
    startScheduledTimer,
    stopTimer,
    getTimerStatus,
    calibrateTimer,
    calibrateAllTimers,
    validateCountdownMinutes,
    validateScheduledTime,
    parseScheduledTime,
    handleTimerUpdate,
    handleTimerExpired,
    resetExpiredTask,
    reStartTimer,
    moveToTrash,
    loadDeletedTasks,
    restoreFromTrash,
    permanentlyDelete,
    clearTrashByPeriod,
    openTrashWindow,
    closeTrashWindow
  }
})