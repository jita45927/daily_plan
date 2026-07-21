<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

interface TaskInfo {
  id: number
  text: string
  status: boolean
  bold: boolean
  color: string
  timerType: string
  timerRemaining: number
}

const task = ref<TaskInfo | null>(null)
const showLimitInput = ref(false)
const limitMinutes = ref('')
const showTimerInput = ref(false)
const showColorPicker = ref(false)

// 预填当前系统时间
const getCurrentDateTimeStr = (): string => {
  const now = new Date()
  const year = now.getFullYear()
  const month = String(now.getMonth() + 1).padStart(2, '0')
  const day = String(now.getDate()).padStart(2, '0')
  const hour = String(now.getHours()).padStart(2, '0')
  const minute = String(now.getMinutes()).padStart(2, '0')
  return `${year}/${month}/${day}-${hour}:${minute}`
}
const timerDateTime = ref(getCurrentDateTimeStr())

const colors = [
  { name: '红色', value: '#EF4444' },
  { name: '绿色', value: '#16A34A' },
  { name: '蓝色', value: '#4444FF' },
  { name: '紫色', value: '#AA44FF' },
  { name: '黑色', value: '#000000' }
]

const menuStyle = {
  width: '100%',
  height: '100%',
  display: 'flex',
  alignItems: 'flex-start',
  justifyContent: 'flex-start',
  padding: '0',
  margin: '0',
  boxSizing: 'border-box' as const,
}

const menuContainerStyle = {
  backgroundColor: '#ffffff',
  borderRadius: '0',
  overflow: 'hidden',
  width: '100%',
  height: '100%',
  padding: '4px 0',
}

const handleMarkCompleted = () => {
  if (task.value) {
    invoke('context_menu_action', { action: 'mark_completed', taskId: task.value.id }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleMarkIncomplete = () => {
  if (task.value) {
    invoke('context_menu_action', { action: 'mark_incomplete', taskId: task.value.id }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleDelete = () => {
  if (task.value) {
    invoke('context_menu_action', { action: 'delete', taskId: task.value.id }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleRestore = () => {
  if (task.value) {
    invoke('context_menu_action', { action: 'restore', taskId: task.value.id }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleLimitTask = () => {
  showLimitInput.value = true
}

const confirmLimitTask = () => {
  const minutes = parseInt(limitMinutes.value)
  if (minutes && minutes > 0 && minutes <= 1440 && task.value) {
    invoke('context_menu_action', { action: 'start_countdown', taskId: task.value.id, value: String(minutes) }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleTimerTask = () => {
  // 重置为当前系统时间，避免上次的输入残留
  timerDateTime.value = getCurrentDateTimeStr()
  showTimerInput.value = true
}

// 聚焦时自动选中 HH:MM 部分，方便用户直接修改时间
const handleTimerFocus = (e: FocusEvent) => {
  const input = e.target as HTMLInputElement
  const dashIndex = input.value.indexOf('-')
  if (dashIndex !== -1) {
    requestAnimationFrame(() => {
      input.setSelectionRange(dashIndex + 1, input.value.length)
    })
  }
}

const confirmTimerTask = () => {
  const dateTime = timerDateTime.value
  if (dateTime && task.value) {
    const match = dateTime.match(/^(\d{4})\/(\d{2})\/(\d{2})-(\d{2}):(\d{2})$/)
    if (match) {
      const [, year, month, day, hour, minute] = match
      const targetTime = new Date(parseInt(year), parseInt(month) - 1, parseInt(day), parseInt(hour), parseInt(minute)).getTime()
      const targetTimestamp = Math.floor(targetTime / 1000)
      invoke('context_menu_action', { action: 'start_scheduled', taskId: task.value.id, value: String(targetTimestamp) }).catch(() => {})
    }
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleCancelTimer = () => {
  if (task.value) {
    invoke('context_menu_action', { action: 'stop_timer', taskId: task.value.id }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleBold = () => {
  if (task.value) {
    invoke('context_menu_action', { action: 'toggle_bold', taskId: task.value.id }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleColor = () => {
  if (task.value && task.value.status) {
    return
  }
  showColorPicker.value = true
}

const selectColor = (color: string) => {
  if (task.value) {
    invoke('context_menu_action', { action: 'set_color', taskId: task.value.id, value: color }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleDefaultStyle = () => {
  if (task.value) {
    invoke('context_menu_action', { action: 'reset_style', taskId: task.value.id }).catch(() => {})
  }
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleClearCompleted = () => {
  invoke('context_menu_action', { action: 'clear_completed', taskId: 0 }).catch(() => {})
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const handleClearAll = () => {
  invoke('context_menu_action', { action: 'clear_all', taskId: 0 }).catch(() => {})
  // 窗口由 Rust 端 context_menu_action 命令自动隐藏
}

const loadTask = async () => {
  try {
    const taskData = await invoke<Record<string, unknown>>('get_context_menu_task')
    if (taskData) {
      task.value = {
        id: taskData.id as number,
        text: taskData.text as string,
        status: taskData.status as boolean,
        bold: taskData.bold as boolean,
        color: taskData.color as string,
        timerType: (taskData.timerType as string) || 'none',
        timerRemaining: (taskData.timerRemaining as number) || 0,
      }
    }
  } catch (err) {
  }
}

let unlistenReload: (() => void) | null = null

onMounted(async () => {
  try {
    await loadTask()
  } catch (err) {
  }

  try {
    unlistenReload = await listen('context-menu-reload', () => {
      showLimitInput.value = false
      showTimerInput.value = false
      showColorPicker.value = false
      loadTask()
    })
  } catch (err) {
  }
})

onUnmounted(() => {
  unlistenReload?.()
})
</script>

<template>
  <div :style="menuStyle">
    <div :style="menuContainerStyle">
      <button
        v-if="task"
        @click="handleMarkCompleted"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#f0fdf4'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        标记已完成
      </button>
      <button
        v-if="task"
        @click="handleMarkIncomplete"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#fef2f2'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        标记未完成
      </button>
      <button
        v-if="task"
        @click="handleDelete"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#fef9c3'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        删除任务
      </button>
      <button
        v-if="task"
        @click="handleRestore"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#f9fafb'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        恢复任务
      </button>

      <div style="height:1px;background-color:#f3f4f6;margin:4px 0;"></div>

      <button
        v-if="task && !task.status"
        @click="handleLimitTask"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#eff6ff'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        限时任务
      </button>
      <button
        v-if="task && !task.status"
        @click="handleTimerTask"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#eff6ff'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        定时任务
      </button>
      <button
        v-if="task"
        @click="handleCancelTimer"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#f9fafb'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        取消定时/限时
      </button>

      <div style="height:1px;background-color:#f3f4f6;margin:4px 0;"></div>

      <button
        v-if="task"
        @click="handleBold"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#faf5ff'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        加粗文字
      </button>
      <button
        v-if="task && !task.status"
        @click="handleColor"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#faf5ff'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        文字改色
      </button>
      <button
        v-if="task"
        @click="handleDefaultStyle"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#f9fafb'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        默认样式
      </button>

      <div style="height:1px;background-color:#f3f4f6;margin:4px 0;"></div>

      <button
        @click="handleClearCompleted"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#fff7ed'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        删除已完成任务
      </button>
      <button
        @click="handleClearAll"
        style="display:block;width:100%;padding:6px 12px;font-size:11px;color:#374151;text-align:left;border:none;background:transparent;cursor:pointer;"
        onmouseover="this.style.backgroundColor='#fef2f2'"
        onmouseout="this.style.backgroundColor='transparent'"
      >
        删除所有任务
      </button>
    </div>

    <!-- 限时任务输入框 -->
    <div
      v-if="showLimitInput"
      style="position:fixed;inset:0;background-color:rgba(0,0,0,0.2);display:flex;align-items:center;justify-content:center;"
      @click.self="showLimitInput = false"
    >
      <div style="background-color:#fff;border-radius:6px;padding:16px;box-shadow:0 4px 12px rgba(0,0,0,0.15);">
        <h3 style="font-size:13px;font-weight:600;color:#1f2937;margin:0 0 8px 0;">限时任务（分钟）</h3>
        <input
          v-model="limitMinutes"
          type="number"
          min="1"
          max="1440"
          placeholder="输入分钟数"
          style="width:100%;padding:6px 8px;border:1px solid #e5e7eb;border-radius:4px;font-size:12px;box-sizing:border-box;"
          @keyup.enter="confirmLimitTask"
        />
        <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:12px;">
          <button
            @click="showLimitInput = false; limitMinutes = ''"
            style="padding:6px 12px;font-size:12px;color:#6b7280;background:transparent;border:none;cursor:pointer;"
          >
            取消
          </button>
          <button
            @click="confirmLimitTask"
            style="padding:6px 12px;font-size:12px;color:#fff;background-color:#eab308;border:none;border-radius:4px;cursor:pointer;"
          >
            确定
          </button>
        </div>
      </div>
    </div>

    <!-- 定时任务输入框 -->
    <div
      v-if="showTimerInput"
      style="position:fixed;inset:0;background-color:rgba(0,0,0,0.2);display:flex;align-items:center;justify-content:center;"
      @click.self="showTimerInput = false"
    >
      <div style="background-color:#fff;border-radius:6px;padding:16px;box-shadow:0 4px 12px rgba(0,0,0,0.15);">
        <h3 style="font-size:13px;font-weight:600;color:#1f2937;margin:0 0 8px 0;">定时任务（YYYY/MM/DD-HH:MM）</h3>
        <input
          v-model="timerDateTime"
          type="text"
          style="width:100%;padding:6px 8px;border:1px solid #e5e7eb;border-radius:4px;font-size:12px;box-sizing:border-box;"
          @keyup.enter="confirmTimerTask"
          @focus="handleTimerFocus"
        />
        <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:12px;">
          <button
            @click="showTimerInput = false; timerDateTime = getCurrentDateTimeStr()"
            style="padding:6px 12px;font-size:12px;color:#6b7280;background:transparent;border:none;border:pointer;"
          >
            取消
          </button>
          <button
            @click="confirmTimerTask"
            style="padding:6px 12px;font-size:12px;color:#fff;background-color:#eab308;border:none;border-radius:4px;cursor:pointer;"
          >
            确定
          </button>
        </div>
      </div>
    </div>

    <!-- 颜色选择器 -->
    <div
      v-if="showColorPicker"
      style="position:fixed;inset:0;background-color:rgba(0,0,0,0.2);display:flex;align-items:center;justify-content:center;"
      @click.self="showColorPicker = false"
    >
      <div style="background-color:#fff;border-radius:6px;padding:16px;box-shadow:0 4px 12px rgba(0,0,0,0.15);">
        <h3 style="font-size:13px;font-weight:600;color:#1f2937;margin:0 0 8px 0;">选择颜色</h3>
        <div style="display:flex;gap:8px;">
          <button
            v-for="color in colors"
            :key="color.value"
            @click="selectColor(color.value)"
            :style="{ backgroundColor: color.value, width: '28px', height: '28px', borderRadius: '50%', border: '2px solid transparent', cursor: 'pointer' }"
            :title="color.name"
            onmouseover="this.style.borderColor='#9ca3af'"
            onmouseout="this.style.borderColor='transparent'"
          ></button>
        </div>
        <button
          @click="showColorPicker = false"
          style="margin-top:12px;width:100%;padding:6px 0;font-size:12px;color:#6b7280;background:transparent;border:none;cursor:pointer;text-align:center;"
        >
          关闭
        </button>
      </div>
    </div>
  </div>
</template>
