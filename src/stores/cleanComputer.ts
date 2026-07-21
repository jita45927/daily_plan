import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { CleanStats } from './types'
import { useAlertsStore } from './alerts'

export const useCleanComputerStore = defineStore('cleanComputer', () => {
  const isCleaningComputer = ref(false)
  const cleanComputerStats = ref<CleanStats | null>(null)
  const cleanComputerNotice = ref({
    show: false,
    title: '',
    message: ''
  })

  const alertsStore = useAlertsStore()

  const formatBytes = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
  }

  const startCleanComputer = async () => {
    if (isCleaningComputer.value) {
      return
    }
    isCleaningComputer.value = true
    cleanComputerStats.value = {
      scanned: 0,
      deleted: 0,
      skipped: 0,
      freedBytes: 0,
      currentCategory: '初始化',
      currentPath: '',
      isRunning: true,
      errorDetails: [],
      categories: []
    }
    try {
      await invoke('clean_computer_cmd')
    } catch (error: any) {
      console.error('[清理电脑] 启动失败:', error)
      isCleaningComputer.value = false
      alertsStore.showErrorAlert(
        '清理失败',
        '启动清理失败:\n' + (error?.message || error?.toString() || '未知错误')
      )
    }
  }

  const handleCleanComputerProgress = (event: { payload: CleanStats }) => {
    cleanComputerStats.value = event.payload
  }

  const handleCleanComputerDone = (event: { payload: CleanStats }) => {
    const stats = event.payload
    cleanComputerStats.value = stats
    isCleaningComputer.value = false

    const categoryLines = stats.categories
      .map(c => `• ${c.name}: 删除 ${c.deleted} 个，释放 ${formatBytes(c.freedBytes)}`)
      .join('\n')

    const message =
      `扫描 ${stats.scanned} 个文件\n` +
      `已删除 ${stats.deleted} 个，跳过 ${stats.skipped} 个（占用/权限）\n` +
      `共释放 ${formatBytes(stats.freedBytes)} 磁盘空间\n\n` +
      `各类别清理情况：\n${categoryLines || '• 无可清理内容'}`

    cleanComputerNotice.value = {
      show: true,
      title: '清理完成',
      message
    }
  }

  const hideCleanComputerNotice = () => {
    cleanComputerNotice.value = { show: false, title: '', message: '' }
  }

  return {
    isCleaningComputer,
    cleanComputerStats,
    cleanComputerNotice,
    formatBytes,
    startCleanComputer,
    handleCleanComputerProgress,
    handleCleanComputerDone,
    hideCleanComputerNotice
  }
})