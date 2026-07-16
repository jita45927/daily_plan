<script setup lang="ts">import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useTaskStore } from '../stores/taskStore';
const taskStore = useTaskStore();
const showLimitInput = ref(false);
const limitMinutes = ref('');
const showTimerInput = ref(false);
const timerDateTime = ref('');
const showColorPicker = ref(false);
const colors = [
 { name: '红色', value: '#FF4444' },
 { name: '绿色', value: '#44FF44' },
 { name: '蓝色', value: '#4444FF' },
 { name: '紫色', value: '#AA44FF' },
 { name: '黑色', value: '#000000' }
];
const currentTask = computed(() => {
 return taskStore.tasks.find(t => t.id === taskStore.contextMenu.taskId);
});
const menuStyle = computed(() => {
 const x = taskStore.contextMenu.x;
 const y = taskStore.contextMenu.y;
 const maxX = window.innerWidth - 560;
 const maxY = window.innerHeight - 280;
 return {
 left: `${Math.min(x, maxX)}px`,
 top: `${Math.min(y, maxY)}px`
 };
});
const handleMarkCompleted = () => {
 if (currentTask.value) {
 taskStore.markCompleted(currentTask.value.id);
 }
 taskStore.closeContextMenu();
};
const handleMarkIncomplete = () => {
 if (currentTask.value) {
 taskStore.markIncomplete(currentTask.value.id);
 }
 taskStore.closeContextMenu();
};
const handleDelete = () => {
 if (currentTask.value) {
 taskStore.showConfirm('删除任务', '是否删除这个任务？', () => {
 taskStore.removeTask(currentTask.value!.id);
 });
 }
 taskStore.closeContextMenu();
};
const handleRestore = () => {
 if (currentTask.value) {
 taskStore.resetTask(currentTask.value.id);
 }
 taskStore.closeContextMenu();
};
const handleLimitTask = () => {
 showLimitInput.value = true;
};
const confirmLimitTask = () => {
 const minutes = parseInt(limitMinutes.value);
 if (minutes && minutes > 0 && minutes <= 1440) {
 if (currentTask.value) {
 taskStore.startCountdown(currentTask.value.id, minutes);
 }
 }
 showLimitInput.value = false;
 limitMinutes.value = '';
 taskStore.closeContextMenu();
};
const handleTimerTask = () => {
 showTimerInput.value = true;
};
const confirmTimerTask = () => {
 const dateTime = timerDateTime.value;
 if (dateTime) {
 const match = dateTime.match(/^(\d{4})\/(\d{2})\/(\d{2})-(\d{2}):(\d{2})$/);
 if (match) {
 const [, year, month, day, hour, minute] = match;
 const targetTime = new Date(parseInt(year), parseInt(month) - 1, parseInt(day), parseInt(hour), parseInt(minute)).getTime();
 const targetTimestamp = Math.floor(targetTime / 1000);
 if (currentTask.value) {
 taskStore.startScheduledTimer(currentTask.value.id, targetTimestamp);
 }
 }
 }
 showTimerInput.value = false;
 timerDateTime.value = '';
 taskStore.closeContextMenu();
};
const handleCancelTimer = () => {
 if (currentTask.value) {
 taskStore.stopTimer(currentTask.value.id);
 }
 taskStore.closeContextMenu();
};
const handleBold = () => {
 if (currentTask.value) {
 taskStore.updateTaskBold(currentTask.value.id, !currentTask.value.bold);
 }
 taskStore.closeContextMenu();
};
const handleColor = () => {
 if (currentTask.value && currentTask.value.status) {
 taskStore.showConfirm('提示', '已完成任务禁止修改文字颜色', () => {});
 return;
 }
 showColorPicker.value = true;
};
const selectColor = (color: string) => {
 if (currentTask.value) {
 taskStore.updateTaskColor(currentTask.value.id, color);
 }
 showColorPicker.value = false;
 taskStore.closeContextMenu();
};
const handleDefaultStyle = () => {
 if (currentTask.value) {
 taskStore.resetTaskStyle(currentTask.value.id);
 }
 taskStore.closeContextMenu();
};
const handleClearCompleted = () => {
 taskStore.showConfirm('删除已完成任务', '是否删除所有已完成任务？', () => {
 taskStore.deleteCompletedTasks();
 });
 taskStore.closeContextMenu();
};
const handleClearAll = () => {
 taskStore.showConfirm('删除所有任务', '是否删除所有任务？', () => {
 taskStore.deleteAllTasks();
 });
 taskStore.closeContextMenu();
};
const handleClickOutside = (e: MouseEvent) => {
 const target = e.target as HTMLElement;
 if (!target.closest('.context-menu') && !target.closest('.context-menu-popup')) {
 taskStore.closeContextMenu();
 showLimitInput.value = false;
 showTimerInput.value = false;
 showColorPicker.value = false;
 }
};
onMounted(() => {
 document.addEventListener('click', handleClickOutside);
});
onUnmounted(() => {
 document.removeEventListener('click', handleClickOutside);
});
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="taskStore.contextMenu.show"
        class="context-menu fixed z-50 bg-white rounded-lg shadow-xl overflow-hidden"
        :style="menuStyle"
      >
        <div class="flex">
          <div class="p-2 border-r border-gray-100">
            <button
              @click="handleMarkCompleted"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-green-50 rounded transition-colors"
            >
              标记已完成
            </button>
            <button
              @click="handleMarkIncomplete"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-red-50 rounded transition-colors"
            >
              标记未完成
            </button>
            <button
              @click="handleDelete"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-yellow-50 rounded transition-colors"
            >
              删除任务
            </button>
            <button
              @click="handleRestore"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 rounded transition-colors"
            >
              恢复任务
            </button>
          </div>

          <div class="p-2 border-r border-gray-100">
            <button
              @click="handleLimitTask"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-blue-50 rounded transition-colors"
            >
              限时任务
            </button>
            <button
              @click="handleTimerTask"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-blue-50 rounded transition-colors"
            >
              定时任务
            </button>
            <button
              @click="handleCancelTimer"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 rounded transition-colors"
            >
              取消定时/限时
            </button>
          </div>

          <div class="p-2 border-r border-gray-100">
            <button
              @click="handleBold"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-purple-50 rounded transition-colors"
            >
              加粗文字
            </button>
            <button
              @click="handleColor"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-purple-50 rounded transition-colors"
            >
              文字改色
            </button>
            <button
              @click="handleDefaultStyle"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 rounded transition-colors"
            >
              默认样式
            </button>
          </div>

          <div class="p-2">
            <button
              @click="handleClearCompleted"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-orange-50 rounded transition-colors"
            >
              删除已完成的任务
            </button>
            <button
              @click="handleClearAll"
              class="block w-full px-3 py-2 text-sm text-gray-700 hover:bg-red-50 rounded transition-colors"
            >
              删除所有任务
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="fade">
      <div
        v-if="showLimitInput"
        class="context-menu-popup fixed inset-0 bg-black/30 flex items-center justify-center z-50"
      >
        <div class="bg-white rounded-lg p-6 shadow-xl">
          <h3 class="text-lg font-semibold text-gray-800 mb-4">限时任务（分钟）</h3>
          <input
            v-model="limitMinutes"
            type="number"
            min="1"
            max="1440"
            placeholder="输入分钟数"
            class="w-full px-3 py-2 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-yellow-500"
          />
          <div class="flex justify-end gap-3 mt-4">
            <button
              @click="showLimitInput = false; limitMinutes = ''; taskStore.closeContextMenu()"
              class="px-4 py-2 text-sm text-gray-600 hover:text-gray-800 transition-colors"
            >
              取消
            </button>
            <button
              @click="confirmLimitTask"
              class="px-4 py-2 text-sm bg-yellow-500 text-white rounded-lg hover:bg-yellow-600 transition-colors"
            >
              确定
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="fade">
      <div
        v-if="showTimerInput"
        class="context-menu-popup fixed inset-0 bg-black/30 flex items-center justify-center z-50"
      >
        <div class="bg-white rounded-lg p-6 shadow-xl">
          <h3 class="text-lg font-semibold text-gray-800 mb-4">定时任务（YYYY/MM/DD-HH:MM）</h3>
          <input
            v-model="timerDateTime"
            type="text"
            placeholder="例如：2024/01/15-14:30"
            class="w-full px-3 py-2 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-yellow-500"
          />
          <div class="flex justify-end gap-3 mt-4">
            <button
              @click="showTimerInput = false; timerDateTime = ''; taskStore.closeContextMenu()"
              class="px-4 py-2 text-sm text-gray-600 hover:text-gray-800 transition-colors"
            >
              取消
            </button>
            <button
              @click="confirmTimerTask"
              class="px-4 py-2 text-sm bg-yellow-500 text-white rounded-lg hover:bg-yellow-600 transition-colors"
            >
              确定
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="fade">
      <div
        v-if="showColorPicker"
        class="context-menu-popup fixed inset-0 bg-black/30 flex items-center justify-center z-50"
      >
        <div class="bg-white rounded-lg p-6 shadow-xl">
          <h3 class="text-lg font-semibold text-gray-800 mb-4">选择颜色</h3>
          <div class="flex gap-3">
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
            @click="showColorPicker = false; taskStore.closeContextMenu()"
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
