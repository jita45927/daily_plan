import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ErrorAlert } from './types'

export const useAlertsStore = defineStore('alerts', () => {
  const confirmDialog = ref({
    show: false,
    title: '',
    message: '',
    onConfirm: () => {},
    onCancel: () => {}
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

  const showErrorAlert = (title: string, message: string) => {
    errorAlert.value = { show: true, title, message }
  }

  const hideErrorAlert = () => {
    errorAlert.value = { show: false, title: '', message: '' }
  }

  const handleDbError = async (error: unknown, action: string) => {
    console.error(`Database error during ${action}:`, error)
    const errorMsg = error instanceof Error ? error.message : String(error)
    
    if (errorMsg.includes('损坏') || errorMsg.includes('corrupted') || errorMsg.includes('CannotOpen')) {
      showErrorAlert('数据库损坏', '检测到数据库文件损坏，已自动重置为空白任务列表。')
      await invoke('reinitialize_db_cmd')
    } else {
      showErrorAlert('数据库错误', `执行${action}时发生错误: ${errorMsg}`)
    }
  }

  const showConfirm = (title: string, message: string, onConfirm: () => void, onCancel?: () => void) => {
    confirmDialog.value = { show: true, title, message, onConfirm, onCancel: onCancel || (() => {}) }
  }

  const hideConfirm = () => {
    confirmDialog.value = { show: false, title: '', message: '', onConfirm: () => {}, onCancel: () => {} }
  }

  const openPopup = (popup: keyof typeof activePopups.value) => {
    activePopups.value[popup] = true
  }

  const closePopup = (popup: keyof typeof activePopups.value) => {
    activePopups.value[popup] = false
  }

  return {
    confirmDialog,
    errorAlert,
    activePopups,
    showErrorAlert,
    hideErrorAlert,
    handleDbError,
    showConfirm,
    hideConfirm,
    openPopup,
    closePopup
  }
})