<script setup lang="ts">
import { computed } from 'vue'
import { useTaskStore } from '../../stores/taskStore'

const taskStore = useTaskStore()

const isAlert = computed(() => {
  return taskStore.confirmDialog.title === '提示'
})

const isDeleteConfirm = computed(() => {
  return taskStore.confirmDialog.title === '删除任务' || taskStore.confirmDialog.title === '删除已完成任务' || taskStore.confirmDialog.title === '删除所有任务' || taskStore.confirmDialog.title === '彻底删除' || taskStore.confirmDialog.title === '清理回收站'
})

const isExitConfirm = computed(() => {
  return taskStore.confirmDialog.title === '退出程序'
})

const handleConfirm = () => {
  taskStore.confirmDialog.onConfirm()
  taskStore.hideConfirm()
}

const handleClose = () => {
  taskStore.hideConfirm()
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="taskStore.confirmDialog.show" class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
        <div class="bg-white rounded-lg p-6 shadow-xl min-w-[280px]">
          <div class="flex items-center gap-3 mb-2" v-if="isAlert">
            <span class="text-2xl">ℹ️</span>
            <h3 class="text-lg font-semibold text-blue-600">
              {{ taskStore.confirmDialog.title }}
            </h3>
          </div>
          <div v-else-if="isDeleteConfirm" class="flex items-center gap-3 mb-2">
            <span class="text-2xl">🗑️</span>
            <h3 class="text-lg font-semibold text-red-600">
              {{ taskStore.confirmDialog.title }}
            </h3>
          </div>
          <h3 v-else class="text-lg font-semibold text-gray-800 mb-2">
            {{ taskStore.confirmDialog.title }}
          </h3>
          <p class="text-sm text-gray-600 mb-4 whitespace-pre-wrap">
            {{ taskStore.confirmDialog.message }}
          </p>
          <div class="flex justify-end gap-3" v-if="isAlert">
            <button
              @click="handleClose"
              class="px-4 py-2 text-sm rounded-lg transition-colors bg-blue-500 hover:bg-blue-600 text-white"
            >
              确定
            </button>
          </div>
          <div class="flex justify-end gap-3" v-else>
            <button
              @click="handleConfirm"
              class="px-4 py-2 text-sm rounded-lg transition-colors"
              :class="isExitConfirm ? 'bg-gray-100 hover:bg-gray-200 text-gray-700' : isDeleteConfirm ? 'bg-gray-100 hover:bg-gray-200 text-gray-700' : 'bg-blue-500 hover:bg-blue-600 text-white'"
            >
              {{ isExitConfirm ? '是' : isDeleteConfirm ? '是' : '确定' }}
            </button>
            <button
              @click="handleClose"
              class="px-4 py-2 text-sm rounded-lg transition-colors bg-red-500 hover:bg-red-600 text-white"
            >
              {{ isExitConfirm ? '否' : isDeleteConfirm ? '否' : '取消' }}
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
