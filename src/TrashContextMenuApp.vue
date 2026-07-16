如“2026/07/17-HH:MM"如“2026/07/17-HH:MM"<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

const taskId = ref<number>(0)

const handlePermanentlyDelete = () => {
  invoke('trash_context_menu_action', { action: 'permanently_delete', taskId: taskId.value }).catch(() => {})
}

const handleRestore = () => {
  invoke('trash_context_menu_action', { action: 'restore', taskId: taskId.value }).catch(() => {})
}

const handleClearOneWeek = () => {
  invoke('trash_context_menu_action', { action: 'clear_one_week', taskId: 0 }).catch(() => {})
}

const handleClearTwoWeeks = () => {
  invoke('trash_context_menu_action', { action: 'clear_two_weeks', taskId: 0 }).catch(() => {})
}

const handleClearMonth = () => {
  invoke('trash_context_menu_action', { action: 'clear_month', taskId: 0 }).catch(() => {})
}

const handleClearAll = () => {
  invoke('trash_context_menu_action', { action: 'clear_all', taskId: 0 }).catch(() => {})
}

const loadTask = async () => {
  try {
    const id = await invoke<number>('get_trash_context_menu_task')
    if (id) {
      taskId.value = id
    }
  } catch (err) {
    console.error('[TrashContextMenuApp] get_trash_context_menu_task error:', err)
  }
}

let unlistenReload: (() => void) | null = null

onMounted(async () => {
  await loadTask()

  unlistenReload = await listen('trash-context-menu-reload', (event: { payload: number }) => {
    if (event.payload) {
      taskId.value = event.payload
    } else {
      loadTask()
    }
  })
})

onUnmounted(() => {
  unlistenReload?.()
})
</script>

<template>
  <div style="width:100%;height:100%;display:flex;align-items:flex-start;justify-content:flex-start;padding:0;margin:0;box-sizing:border-box;">
    <div style="background-color:#ffffff;border-radius:0;overflow:hidden;width:100%;height:100%;padding:4px 0;">

      <button
        @click="handlePermanentlyDelete"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#EF4444;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#fef2f2'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        彻底清除该任务
      </button>
      <button
        @click="handleRestore"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#f0fdf4'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        恢复该任务
      </button>

      <div style="height:1px;background-color:#f3f4f6;margin:4px 0;"></div>

      <div style="padding:4px 12px 2px 12px;font-size:10px;color:#9ca3af;font-weight:600;">
        清理回收站
      </div>
      <button
        @click="handleClearOneWeek"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#f9fafb'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        一周前
      </button>
      <button
        @click="handleClearTwoWeeks"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#f9fafb'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        两周前
      </button>
      <button
        @click="handleClearMonth"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#f9fafb'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        一个月前
      </button>
      <button
        @click="handleClearAll"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#EF4444;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#fef2f2'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        清理全部
      </button>

    </div>
  </div>
</template>
