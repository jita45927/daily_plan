<script setup lang="ts">
import { computed } from 'vue'
import { useTaskStore } from '../../stores/taskStore'

const taskStore = useTaskStore()

const expiredTask = computed(() => taskStore.expiredTask)

const handleMarkCompleted = () => {
  if (expiredTask.value) {
    taskStore.markCompleted(expiredTask.value.task_id)
    taskStore.resetExpiredTask()
  }
}

const handleMarkIncomplete = () => {
  if (expiredTask.value) {
    taskStore.markIncomplete(expiredTask.value.task_id)
    taskStore.resetExpiredTask()
  }
}

const handleReStart = () => {
  taskStore.reStartTimer()
}

const handleCancel = () => {
  taskStore.resetExpiredTask()
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
              @click="handleReStart"
              class="w-full py-2.5 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors flex items-center justify-center gap-2"
            >
              <span>🔄</span>
              <span>重新计时</span>
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