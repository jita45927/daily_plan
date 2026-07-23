<script setup lang="ts">
import { computed, ref } from 'vue'
import { useTaskStore } from '../../stores/taskStore'
import { invoke } from '@tauri-apps/api/core'

const taskStore = useTaskStore()

const expiredTask = computed(() => taskStore.expiredTask)
const showLimitInput = ref(false)
const limitMinutes = ref(25)

const stopAlarm = () => {
  invoke('stop_alarm_cmd').catch(() => {})
}

const handleMarkCompleted = () => {
  stopAlarm()
  if (expiredTask.value) {
    taskStore.markCompleted(expiredTask.value.task_id)
    taskStore.resetExpiredTask()
  }
}

const handleMarkIncomplete = () => {
  stopAlarm()
  if (expiredTask.value) {
    taskStore.markIncomplete(expiredTask.value.task_id)
    taskStore.resetExpiredTask()
  }
}

const handleReStart = () => {
  stopAlarm()
  showLimitInput.value = true
}

const confirmReStart = () => {
  const minutes = parseInt(String(limitMinutes.value))
  if (minutes && minutes > 0 && minutes <= 1440) {
    taskStore.reStartTimer(minutes)
  }
  showLimitInput.value = false
  limitMinutes.value = 25
}

const cancelReStart = () => {
  showLimitInput.value = false
  limitMinutes.value = 25
}

const handleCancel = () => {
  stopAlarm()
  taskStore.resetExpiredTask()
}

const handleStopAlarm = () => {
  stopAlarm()
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="taskStore.activePopups.countdownAlert && expiredTask" class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
        <div class="bg-white rounded-lg p-6 shadow-xl min-w-[280px]">
          <div class="text-center mb-4">
            <div class="text-4xl mb-2">🔔</div>
            <h3 class="text-lg font-semibold text-gray-800">任务时间到！</h3>
            <p class="text-sm text-gray-600 mt-1">{{ expiredTask.task_title }}</p>
          </div>
          
          <div class="space-y-2">
            <button
              @click="handleMarkCompleted"
              class="w-full py-2.5 text-sm bg-green-500 text-white rounded-lg hover:bg-green-600 transition-colors flex items-center justify-center gap-2"
            >
              <span>✓</span>
              <span>标记已完成</span>
            </button>
            
            <button
              @click="handleMarkIncomplete"
              class="w-full py-2.5 text-sm bg-red-500 text-white rounded-lg hover:bg-red-600 transition-colors flex items-center justify-center gap-2"
            >
              <span>✕</span>
              <span>标记未完成</span>
            </button>
            
            <button
              v-if="!showLimitInput"
              @click="handleReStart"
              class="w-full py-2.5 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors flex items-center justify-center gap-2"
            >
              <span>🔄</span>
              <span>重新计时</span>
            </button>
            
            <div v-else class="bg-gray-50 rounded-lg p-3">
              <div class="flex items-center gap-2">
                <span class="text-sm text-gray-600">限时（分钟）：</span>
                <input
                  id="restart-limit-minutes"
                  name="restart-limit-minutes"
                  v-model.number="limitMinutes"
                  type="number"
                  min="1"
                  max="1440"
                  class="flex-1 px-3 py-2 text-center border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-yellow-500"
                  autocomplete="off"
                />
              </div>
              <div class="flex justify-end gap-2 mt-2">
                <button
                  @click="cancelReStart"
                  class="px-3 py-1.5 text-xs text-gray-600 hover:text-gray-800 transition-colors"
                >
                  取消
                </button>
                <button
                  @click="confirmReStart"
                  class="px-3 py-1.5 text-xs bg-yellow-500 text-white rounded-lg hover:bg-yellow-600 transition-colors"
                >
                  确定
                </button>
              </div>
            </div>
            
            <button
              @click="handleStopAlarm"
              class="w-full py-2.5 text-sm bg-orange-500 text-white rounded-lg hover:bg-orange-600 transition-colors flex items-center justify-center gap-2"
            >
              <span>🔇</span>
              <span>临时静音</span>
            </button>
            
            <button
              @click="handleCancel"
              class="w-full py-2.5 text-sm bg-gray-200 text-gray-600 rounded-lg hover:bg-gray-300 transition-colors"
            >
              取消
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