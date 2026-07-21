import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { TimerState, ExpiredTask, Task } from './types'
import { useAlertsStore } from './alerts'
import { useTasksStore } from './tasks'

export const useTimersStore = defineStore('timers', () => {
  const timerStates = ref<Map<number, TimerState>>(new Map())
  const expiredTask = ref<ExpiredTask | null>(null)
  const isMuted = ref(true)

  const alertsStore = useAlertsStore()

  const restoreTimers = async (tasks: Task[]) => {
    const timerTasks = tasks.filter(t => t.timerType && t.timerType !== '' && t.timerValue > 0)
    
    if (timerTasks.length === 0) {
      return
    }

    const currentTime = Math.floor(Date.now() / 1000)
    const promises: Promise<void>[] = []

    for (const task of timerTasks) {
      if (currentTime >= task.timerValue) {
        task.timerRemaining = 0
        task.timerType = ''
        task.timerValue = 0
        task.status = true
        task.color = '#000000'
        task.bold = false
        promises.push(
          invoke('update_task_cmd', {
            id: task.id,
            text: task.text,
            status: true,
            color: '#000000',
            bold: false,
            timerType: '',
            timerValue: 0,
            timerRemaining: 0
          }).then(() => {})
        )
      } else {
        const remainingSeconds = task.timerValue - currentTime
        task.timerRemaining = remainingSeconds
        promises.push(
          invoke('update_task_cmd', {
            id: task.id,
            text: task.text,
            status: task.status,
            color: task.color,
            bold: task.bold,
            timerType: task.timerType,
            timerValue: task.timerValue,
            timerRemaining: remainingSeconds
          }).then(() => {
            return invoke('restore_scheduled_timer_cmd', {
              taskId: task.id,
              targetTimestamp: task.timerValue
            }).then(() => {})
          })
        )
      }
    }

    await Promise.all(promises)
  }

  const startCountdown = async (taskId: number, minutes: number) => {
    const tasksStore = useTasksStore()
    const validation = tasksStore.validateCountdownMinutes(minutes.toString())
    if (!validation.valid) {
      alertsStore.showErrorAlert('输入错误', validation.message)
      return
    }

    try {
      await invoke('start_countdown_cmd', { taskId, minutes })
      const task = tasksStore.tasks.find(t => t.id === taskId)
      if (task) {
        task.timerType = 'countdown'
        const targetTimestamp = Math.floor(Date.now() / 1000) + minutes * 60
        task.timerValue = targetTimestamp
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
          timerValue: targetTimestamp,
          timerRemaining: minutes * 60
        })
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error)
      alertsStore.showErrorAlert('启动倒计时失败', errorMsg)
    }
  }

  const startScheduledTimer = async (taskId: number, targetTimestamp: number) => {
    const tasksStore = useTasksStore()
    try {
      const result = await invoke('start_scheduled_timer_cmd', { taskId, targetTimestamp: targetTimestamp })
      if (result === '目标时间必须大于当前时间') {
        alertsStore.showErrorAlert('输入错误', '目标时间不能早于当前时间')
        return
      }
      
      const task = tasksStore.tasks.find(t => t.id === taskId)
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
      alertsStore.showErrorAlert('启动定时任务失败', errorMsg)
    }
  }

  const stopTimer = async (taskId: number) => {
    try {
      await invoke('stop_timer_cmd', { taskId: taskId })
      timerStates.value.delete(taskId)
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
    const tasksStore = useTasksStore()
    try {
      const status = await invoke<TimerState | null>('calibrate_timer_cmd', { taskId })
      if (status) {
        timerStates.value.set(taskId, status)
        const task = tasksStore.tasks.find(t => t.id === taskId)
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
    const tasksStore = useTasksStore()
    const activeTimers = tasksStore.tasks.filter(t => t.timerType && t.timerRemaining > 0)
    if (activeTimers.length === 0) return
    await Promise.all(activeTimers.map(t => calibrateTimer(t.id)))
  }

  const handleTimerUpdate = (event: { payload: { task_id: number; remaining: number; hours: number; minutes: number; seconds: number; formatted: string } }) => {
    const tasksStore = useTasksStore()
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

    const task = tasksStore.tasks.find(t => t.id === task_id)
    if (task) {
      task.timerRemaining = remaining
    }
  }

  const handleTimerExpired = async (event: { payload: { task_id: number; timerType: string } }) => {
    const tasksStore = useTasksStore()
    const alertsStore = useAlertsStore()
    const { task_id, timerType } = event.payload
    timerStates.value.delete(task_id)

    const task = tasksStore.tasks.find(t => t.id === task_id)
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
      alertsStore.openPopup('countdownAlert')

      if (!isMuted.value) {
        await invoke('play_alarm_cmd').catch(() => {})
      }
    }
  }

  const toggleMute = () => {
    isMuted.value = !isMuted.value
  }

  const resetExpiredTask = () => {
    const alertsStore = useAlertsStore()
    expiredTask.value = null
    alertsStore.closePopup('countdownAlert')
  }

  const reStartTimer = async () => {
    if (!expiredTask.value) return

    const { task_id, timerType, lastTimerValue } = expiredTask.value
    if (timerType === 'countdown') {
      const minutes = Math.floor(lastTimerValue / 60)
      if (minutes > 0) {
        await startCountdown(task_id, minutes)
      }
    } else if (timerType === 'scheduled') {
      const currentTime = Math.floor(Date.now() / 1000)
      const newTarget = currentTime + lastTimerValue
      await startScheduledTimer(task_id, newTarget)
    }
    resetExpiredTask()
  }

  return {
    timerStates,
    expiredTask,
    isMuted,
    restoreTimers,
    startCountdown,
    startScheduledTimer,
    stopTimer,
    getTimerStatus,
    calibrateTimer,
    calibrateAllTimers,
    handleTimerUpdate,
    handleTimerExpired,
    resetExpiredTask,
    reStartTimer,
    toggleMute
  }
})