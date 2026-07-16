<script setup lang="ts">
import { computed } from 'vue'
import { useTaskStore } from '../../stores/taskStore'

const taskStore = useTaskStore()

const colors = [
  { name: '红色', value: '#FF4444' },
  { name: '绿色', value: '#44FF44' },
  { name: '蓝色', value: '#4444FF' },
  { name: '紫色', value: '#AA44FF' },
  { name: '黑色', value: '#000000' }
]

const currentTask = computed(() => {
  return taskStore.tasks.find(t => t.id === taskStore.contextMenu.taskId)
})

const emit = defineEmits<{
  (e: 'select', color: string): void
}>()

const selectColor = (color: string) => {
  if (currentTask.value && currentTask.value.status) {
    taskStore.showConfirm('提示', '已完成任务禁止修改文字颜色', () => {})
    return
  }
  emit('select', color)
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="taskStore.activePopups.colorPicker" class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
        <div class="bg-white rounded-lg p-6 shadow-xl">
          <h3 class="text-lg font-semibold text-gray-800 mb-4">选择颜色</h3>
          <div class="flex gap-4 justify-center">
            <button
              v-for="color in colors"
              :key="color.value"
              @click="selectColor(color.value)"
              :style="{ backgroundColor: color.value }"
              class="w-10 h-10 rounded-full border-2 border-transparent hover:border-gray-400 transition-all"
              :title="color.name"
            ></button>
          </div>
          <button
            @click="taskStore.closePopup('colorPicker')"
            class="mt-4 w-full py-2 text-sm text-gray-600 hover:text-gray-800 transition-colors"
          >
            关闭
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
