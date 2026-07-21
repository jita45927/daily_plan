<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { useTaskStore } from '../stores/taskStore'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

const taskStore = useTaskStore()

const menuStyle = computed(() => {
  const x = taskStore.mainMenu.x
  const y = taskStore.mainMenu.y
  const maxX = window.innerWidth - 160
  const maxY = window.innerHeight - 250
  return {
    left: `${Math.min(x, maxX)}px`,
    top: `${Math.min(y, maxY)}px`
  }
})

const handleCleanComputer = () => {
  taskStore.closeMainMenu()

  if (taskStore.isCleaningComputer) {
    taskStore.showErrorAlert('提示', '清理任务正在进行中，请等待完成。')
    return
  }

  taskStore.showConfirm(
    '清理电脑',
    '将安全清理以下垃圾文件（后台执行，不影响其他功能）：\n\n' +
    '• 用户临时文件（7 天以上）\n' +
    '• 系统临时文件（7 天以上）\n' +
    '• 浏览器缓存（Edge/Chrome/Firefox）\n' +
    '• Windows 更新缓存\n' +
    '• 缩略图缓存\n' +
    '• 系统日志（7 天以上）\n\n' +
    '安全保证：\n' +
    '• 仅清理白名单目录，不删除用户文档\n' +
    '• 占用/权限不足的文件自动跳过\n' +
    '• 清理在后台线程执行，主窗口可继续使用\n\n' +
    '点击"确定"开始清理。',
    () => {
      taskStore.startCleanComputer()
    }
  )
}

const handleOrganizeDesktop = async () => {
  taskStore.closeMainMenu()
  // 第一步：显示 loading，后台执行分析（不创建窗口，避免焦点转移导致主窗口收起）
  taskStore.isAnalyzingDesktop = true
  try {
    await invoke('analyze_desktop_cmd')
  } catch (error: any) {
    console.error('[整理桌面] 分析桌面失败:', error)
    alert('分析桌面失败:\n' + (error?.message || error?.toString() || '未知错误'))
    return
  } finally {
    taskStore.isAnalyzingDesktop = false
  }
  // 第二步：分析完成后，显示结果窗口
  try {
    await invoke('show_analyze_window')
  } catch (error: any) {
    console.error('[整理桌面] 显示分析窗口失败:', error)
    alert('显示分析窗口失败:\n' + (error?.message || error?.toString() || '未知错误'))
  }
}

const handleCleanDuplicateFiles = () => {
  taskStore.closeMainMenu()
  
  taskStore.showConfirm(
    '清理重复文件',
    '确定要清理桌面上的重复文件吗？\n\n' +
    '将扫描以下文件夹中的文件：\n' +
    '• 程序快捷方式\n' +
    '• 其他快捷方式\n' +
    '• 桌面整理文件\n' +
    '• 桌面图片文件\n\n' +
    '对于内容完全相同但名称不同的文件：\n' +
    '• 保留按名称排序的第一个文件\n' +
    '• 其余文件将被移到回收站\n\n' +
    '注意：只扫描文件，不扫描文件夹。',
    async () => {
      taskStore.isCleaningDuplicates = true
      try {
        const result = await invoke<[number, number, string[]]>('clean_duplicate_files_cmd')
        const [groups, moved, errors] = result
        let msg = `清理完成！\n\n发现 ${groups} 组重复文件\n已移入回收站 ${moved} 个文件`
        if (errors.length > 0) {
          msg += `\n\n错误 ${errors.length} 个：\n${errors.slice(0, 5).join('\n')}`
        }
        if (groups === 0) {
          msg = '未发现重复文件。\n\n请确认已使用"整理桌面"功能将文件分类到四个文件夹中。'
        }
        taskStore.showConfirm('提示', msg, () => {})
      } catch (error: any) {
        console.error('[清理重复文件] 失败:', error)
        taskStore.showErrorAlert(
          '清理失败',
          '清理重复文件失败:\n' + (error?.message || error?.toString() || '未知错误')
        )
      } finally {
        taskStore.isCleaningDuplicates = false
      }
    }
  )
}

const handleCleanFolderDuplicates = async () => {
  taskStore.closeMainMenu()

  let targetPath: string | null = null
  try {
    const downloadsPath = await invoke<string>('get_downloads_path_cmd')
    targetPath = downloadsPath
  } catch {
    targetPath = null
  }

  const useDefault = await new Promise<boolean>((resolve) => {
    const dialog = document.createElement('div')
    dialog.style.cssText = `
      position: fixed;
      top: 0; left: 0; right: 0; bottom: 0;
      background: rgba(0,0,0,0.5);
      display: flex; align-items: center; justify-content: center;
      z-index: 10000;
    `
    dialog.innerHTML = `
      <div style="background: white; padding: 20px; border-radius: 8px; min-width: 360px; box-shadow: 0 10px 30px rgba(0,0,0,0.2);">
        <h3 style="margin: 0 0 12px 0; font-size: 16px;">清理文件夹重复文件</h3>
        <div style="margin-bottom: 16px; font-size: 13px; color: #333;">
          <div style="margin-bottom: 8px;">默认清理系统下载目录：</div>
          <code style="background: #f3f4f6; padding: 4px 8px; border-radius: 4px; font-size: 12px; display: block; word-break: break-all;">${targetPath || '无法获取下载目录'}</code>
          <div style="margin-top: 8px; color: #666;">或选择其他文件夹进行清理</div>
        </div>
        <div style="display: flex; gap: 8px;">
          <button id="btn-default" style="flex: 1; padding: 8px 16px; background: #2563eb; color: white; border: none; border-radius: 4px; cursor: pointer;">使用默认目录</button>
          <button id="btn-select" style="flex: 1; padding: 8px 16px; background: #fff; color: #333; border: 1px solid #ddd; border-radius: 4px; cursor: pointer;">选择其他文件夹</button>
          <button id="btn-cancel" style="padding: 8px 16px; background: #6b7280; color: white; border: none; border-radius: 4px; cursor: pointer;">取消</button>
        </div>
      </div>
    `
    document.body.appendChild(dialog)

    dialog.querySelector('#btn-default')?.addEventListener('click', () => {
      document.body.removeChild(dialog)
      resolve(true)
    })

    dialog.querySelector('#btn-select')?.addEventListener('click', async () => {
      document.body.removeChild(dialog)
      try {
        const selected = await open({
          directory: true,
          multiple: false,
          title: '选择要清理重复文件的文件夹',
        })
        if (selected) {
          targetPath = typeof selected === 'string' ? selected : selected[0]
          resolve(true)
        } else {
          resolve(false)
        }
      } catch {
        resolve(false)
      }
    })

    dialog.querySelector('#btn-cancel')?.addEventListener('click', () => {
      document.body.removeChild(dialog)
      resolve(false)
    })
  })

  if (!useDefault || !targetPath) {
    return
  }

  taskStore.showConfirm(
    '清理文件夹重复文件',
    `确定要清理以下文件夹中的重复文件吗？\n\n` +
    `目录：${targetPath}\n\n` +
    '将扫描以下子文件夹中的文件：\n' +
    '• 可执行文件\n' +
    '• 图片文件\n' +
    '• 其他文件\n' +
    '• 压缩包\n\n' +
    '对于内容完全相同但名称不同的文件：\n' +
    '• 保留按名称排序的第一个文件\n' +
    '• 其余文件将被移到回收站\n\n' +
    '注意：只扫描文件，不扫描文件夹。',
    async () => {
      taskStore.isCleaningDuplicates = true
      try {
        const result = await invoke<[number, number, string[]]>('clean_duplicate_files_for_folder_cmd', { folderPath: targetPath })
        const [groups, moved, errors] = result
        let msg = `清理完成！\n\n发现 ${groups} 组重复文件\n已移入回收站 ${moved} 个文件`
        if (errors.length > 0) {
          msg += `\n\n错误 ${errors.length} 个：\n${errors.slice(0, 5).join('\n')}`
        }
        if (groups === 0) {
          msg = '未发现重复文件。\n\n请确认已使用"整理文件夹"功能将文件分类到四个文件夹中。'
        }
        taskStore.showConfirm('提示', msg, () => {})
      } catch (error: any) {
        console.error('[清理文件夹重复文件] 失败:', error)
        taskStore.showErrorAlert(
          '清理失败',
          '清理文件夹重复文件失败:\n' + (error?.message || error?.toString() || '未知错误')
        )
      } finally {
        taskStore.isCleaningDuplicates = false
      }
    }
  )
}

const handleEmptyRecycleBin = async () => {
  taskStore.closeMainMenu()
  try {
    await invoke('empty_recycle_bin_cmd')
  } catch (error: any) {
    console.error('[清空回收站] 失败:', error)
    alert('清空回收站失败:\n' + (error?.message || error?.toString() || '未知错误'))
  }
}

const handleOrganizeDownloads = async () => {
  taskStore.closeMainMenu()

  taskStore.isAnalyzingDesktop = true
  try {
    await invoke('analyze_downloads_cmd')
  } catch (error: any) {
    console.error('[整理文件夹] 分析目录失败:', error)
    alert('分析目录失败:\n' + (error?.message || error?.toString() || '未知错误'))
    return
  } finally {
    taskStore.isAnalyzingDesktop = false
  }

  try {
    await invoke('show_downloads_analyze_window')
  } catch (error: any) {
    console.error('[整理文件夹] 显示分析窗口失败:', error)
    alert('显示分析窗口失败:\n' + (error?.message || error?.toString() || '未知错误'))
  }
}

const handleContextMenu = (e: MouseEvent) => {
  if (taskStore.mainMenu.show) {
    e.preventDefault()
    taskStore.closeMainMenu()
  }
}

onMounted(() => {
  document.addEventListener('contextmenu', handleContextMenu)
})

onUnmounted(() => {
  document.removeEventListener('contextmenu', handleContextMenu)
})
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="taskStore.mainMenu.show"
        class="main-menu fixed z-50 bg-white rounded-lg shadow-xl overflow-hidden"
        :style="menuStyle"
      >
        <div class="p-2">
          <button
            @click="handleCleanComputer"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-blue-50 rounded transition-colors"
          >
            清理电脑
          </button>
          <button
            @click="handleOrganizeDesktop"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-blue-50 rounded transition-colors"
          >
            整理桌面文件
          </button>
          <button
            @click="handleOrganizeDownloads"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-blue-50 rounded transition-colors"
          >
            整理文件夹
          </button>
          <button
            @click="handleCleanDuplicateFiles"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-orange-50 rounded transition-colors"
          >
            清理桌面重复文件
          </button>
          <button
            @click="handleCleanFolderDuplicates"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-orange-50 rounded transition-colors"
          >
            清理文件夹重复文件
          </button>
          <button
            @click="handleEmptyRecycleBin"
            class="block w-full px-4 py-2 text-sm text-gray-700 hover:bg-red-50 rounded transition-colors"
          >
            清空回收站
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
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
</style>