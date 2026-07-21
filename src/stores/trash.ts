import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { DeletedTask } from './types'
import { useAlertsStore } from './alerts'
import { useTimersStore } from './timers'
import { useTasksStore } from './tasks'

type DeletedTaskResponse = DeletedTask

export const useTrashStore = defineStore('trash', () => {
  const deletedTasks = ref<DeletedTask[]>([])
  const trashWindowVisible = ref(false)

  const alertsStore = useAlertsStore()
  const timersStore = useTimersStore()
  const tasksStore = useTasksStore()

  const moveToTrash = async (id: number) => {
    try {
      await invoke('stop_timer_cmd', { taskId: id })
      timersStore.timerStates.delete(id)
      await invoke('move_task_to_trash_cmd', { taskId: id })
      const index = tasksStore.tasks.findIndex(t => t.id === id)
      if (index !== -1) {
        tasksStore.tasks.splice(index, 1)
      }
    } catch (error) {
      await alertsStore.handleDbError(error, '移到回收站')
    }
  }

  const loadDeletedTasks = async () => {
    try {
      const result = await invoke<DeletedTaskResponse[]>('get_deleted_tasks_cmd')
      deletedTasks.value = result
    } catch (error) {
      await alertsStore.handleDbError(error, '加载回收站')
    }
  }

  const restoreFromTrash = async (deletedId: number) => {
    try {
      await invoke('restore_task_cmd', { deletedId })
      const index = deletedTasks.value.findIndex(t => t.id === deletedId)
      if (index !== -1) {
        deletedTasks.value.splice(index, 1)
      }
      await tasksStore.loadTasks()
    } catch (error) {
      await alertsStore.handleDbError(error, '恢复任务')
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
      await alertsStore.handleDbError(error, '彻底删除任务')
    }
  }

  const clearTrashByPeriod = async (periodDays: number) => {
    try {
      await invoke('clear_trash_by_period_cmd', { periodDays })
      await loadDeletedTasks()
    } catch (error) {
      await alertsStore.handleDbError(error, '清理回收站')
    }
  }

  const openTrashWindow = () => {
    trashWindowVisible.value = true
    loadDeletedTasks()
  }

  const closeTrashWindow = () => {
    trashWindowVisible.value = false
  }

  return {
    deletedTasks,
    trashWindowVisible,
    moveToTrash,
    loadDeletedTasks,
    restoreFromTrash,
    permanentlyDelete,
    clearTrashByPeriod,
    openTrashWindow,
    closeTrashWindow
  }
})