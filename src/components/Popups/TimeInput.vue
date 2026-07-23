<script setup lang="ts">
import { ref } from 'vue'
import { useTaskStore } from '../../stores/taskStore'

const taskStore = useTaskStore()

const props = defineProps<{
  mode: 'countdown' | 'scheduled'
}>()

const emit = defineEmits<{
  (e: 'select', value: number): void
}>()

const hours = ref('')
const minutes = ref('')
const errorMessage = ref('')

const getCurrentDateTimeStr = (): string => {
  const now = new Date()
  const year = now.getFullYear()
  const month = String(now.getMonth() + 1).padStart(2, '0')
  const day = String(now.getDate()).padStart(2, '0')
  const hour = String(now.getHours()).padStart(2, '0')
  const minute = String(now.getMinutes()).padStart(2, '0')
  return `${year}/${month}/${day}-${hour}:${minute}`
}

// 预填当前系统时间，用户可直接修改
const scheduledTime = ref(getCurrentDateTimeStr())

const validateCountdown = (): boolean => {
  errorMessage.value = ''
  
  if (!hours.value && !minutes.value) {
    errorMessage.value = '请输入小时或分钟'
    return false
  }

  const h = parseInt(hours.value) || 0
  const m = parseInt(minutes.value) || 0
  
  if (isNaN(h) || isNaN(m)) {
    errorMessage.value = '请输入有效的数字'
    return false
  }

  if (h < 0 || h > 24) {
    errorMessage.value = '小时数必须在 0-24 之间'
    return false
  }

  if (m < 0 || m > 59) {
    errorMessage.value = '分钟数必须在 0-59 之间'
    return false
  }

  const totalMinutes = h * 60 + m
  
  if (totalMinutes < 1) {
    errorMessage.value = '总时长至少为 1 分钟'
    return false
  }

  if (totalMinutes > 1440) {
    errorMessage.value = '总时长不能超过 1440 分钟（24小时）'
    return false
  }

  return true
}

const validateScheduled = (): boolean => {
  errorMessage.value = ''
  
  if (!scheduledTime.value) {
    errorMessage.value = '请输入目标时间'
    return false
  }

  const validation = taskStore.validateScheduledTime(scheduledTime.value)
  if (!validation.valid) {
    errorMessage.value = validation.message
    return false
  }

  return true
}

const handleConfirm = () => {
  if (props.mode === 'countdown') {
    if (!validateCountdown()) return
    
    const h = parseInt(hours.value) || 0
    const m = parseInt(minutes.value) || 0
    const totalMinutes = h * 60 + m
    
    emit('select', totalMinutes)
    taskStore.closePopup('timeInput')
  } else {
    if (!validateScheduled()) return
    
    const timestamp = taskStore.parseScheduledTime(scheduledTime.value)
    if (timestamp) {
      emit('select', timestamp)
      taskStore.closePopup('timeInput')
    }
  }
}

const handleInput = () => {
  errorMessage.value = ''
}

// 聚焦时自动选中 HH:MM 部分，方便用户直接修改时间
const handleScheduledFocus = (e: FocusEvent) => {
  const input = e.target as HTMLInputElement
  const dashIndex = input.value.indexOf('-')
  if (dashIndex !== -1) {
    // 延迟选中以确保 DOM 已更新
    requestAnimationFrame(() => {
      input.setSelectionRange(dashIndex + 1, input.value.length)
    })
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="taskStore.activePopups.timeInput" class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
        <div class="bg-white rounded-lg p-6 shadow-xl w-80">
          <h3 class="text-lg font-semibold text-gray-800 mb-4">
            {{ mode === 'countdown' ? '设置限时任务' : '设置定时任务' }}
          </h3>
          
          <div v-if="mode === 'countdown'" class="space-y-4">
            <div class="flex items-center gap-2">
              <input
                id="countdown-hours"
                name="countdown-hours"
                v-model="hours"
                type="number"
                min="0"
                max="24"
                placeholder="时"
                class="w-20 px-3 py-2 text-center border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-yellow-500"
                @input="handleInput"
                autocomplete="off"
              />
              <span class="text-gray-400">:</span>
              <input
                id="countdown-minutes"
                name="countdown-minutes"
                v-model="minutes"
                type="number"
                min="0"
                max="59"
                placeholder="分"
                class="w-20 px-3 py-2 text-center border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-yellow-500"
                @input="handleInput"
                autocomplete="off"
              />
            </div>
            <p class="text-xs text-gray-500">提示：总时长范围 1分钟-24:00</p>
          </div>
          
          <div v-else class="space-y-4">
            <input
              id="scheduled-time"
              name="scheduled-time"
              v-model="scheduledTime"
              type="text"
              class="w-full px-3 py-2 text-center border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-yellow-500"
              @input="handleInput"
              @focus="handleScheduledFocus"
              autocomplete="off"
            />
            <p class="text-xs text-gray-500">格式：YYYY/MM/DD-HH:MM，不能早于当前时间</p>
          </div>
          
          <div v-if="errorMessage" class="mt-3 p-2 bg-red-50 text-red-600 text-sm rounded-lg">
            {{ errorMessage }}
          </div>
          
          <div class="flex justify-end gap-3 mt-4">
            <button
              @click="taskStore.closePopup('timeInput')"
              class="px-4 py-2 text-sm text-gray-600 hover:text-gray-800 transition-colors"
            >
              取消
            </button>
            <button
              @click="handleConfirm"
              class="px-4 py-2 text-sm bg-yellow-500 text-white rounded-lg hover:bg-yellow-600 transition-colors"
            >
              确定
            </button>
          </div>
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