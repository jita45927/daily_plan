<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'

interface DownloadsItem {
  name: string
  category: string
  path: string | null
  isDir: boolean
}

interface DownloadsAnalysis {
  downloadsPath: string
  items: DownloadsItem[]
  exeCount: number
  imageCount: number
  archiveCount: number
  otherCount: number
  errors: string[]
}

const analysis = ref<DownloadsAnalysis | null>(null)
const loading = ref(false)
const organizing = ref(false)
const errorMsg = ref('')
const activeCategory = ref<string | 'all'>('all')
const unlisteners: Array<() => void> = []
const showConflictDialog = ref(false)
const conflictList = ref<Array<{fileName: string, sourcePath: string, targetFolder: string, targetPath: string}>>([])
let conflictResolver: ((strategy: 'Overwrite' | 'Rename' | 'Skip' | 'Cancel') => void) | null = null
const customPath = ref<string | null>(null)

const selectFolder = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择要分析的文件夹',
    })
    if (selected) {
      customPath.value = typeof selected === 'string' ? selected : selected[0]
      await reanalyze()
    }
  } catch (e: any) {
    console.error('[文件夹分析] 选择文件夹失败:', e)
    errorMsg.value = '选择文件夹失败: ' + (e?.message || e?.toString() || '未知错误')
  }
}

const categoryMeta: Record<string, { label: string; color: string; bg: string }> = {
  exe: { label: '可执行文件', color: '#DC2626', bg: '#FEF2F2' },
  image: { label: '图片文件', color: '#DB2777', bg: '#FCE7F3' },
  archive: { label: '压缩包', color: '#2563EB', bg: '#DBEAFE' },
  other: { label: '其他文件和文件夹', color: '#16A34A', bg: '#DCFCE7' },
}

const totalCount = computed(() => analysis.value?.items.length ?? 0)

const filteredItems = computed(() => {
  if (!analysis.value) return []
  if (activeCategory.value === 'all') return analysis.value.items
  return analysis.value.items.filter(i => i.category === activeCategory.value)
})

const statsList = computed(() => {
  if (!analysis.value) return []
  return [
    { key: 'all', label: '全部', count: totalCount.value, color: '#374151', bg: '#F3F4F6' },
    { key: 'exe', label: '可执行文件', count: analysis.value.exeCount, color: '#DC2626', bg: '#FEF2F2' },
    { key: 'image', label: '图片文件', count: analysis.value.imageCount, color: '#DB2777', bg: '#FCE7F3' },
    { key: 'archive', label: '压缩包', count: analysis.value.archiveCount, color: '#2563EB', bg: '#DBEAFE' },
    { key: 'other', label: '其他文件和文件夹', count: analysis.value.otherCount, color: '#16A34A', bg: '#DCFCE7' },
  ]
})

const loadAnalysis = async () => {
  loading.value = true
  errorMsg.value = ''
  try {
    const result = await invoke<DownloadsAnalysis | null>('get_downloads_analysis')
    if (result) {
      analysis.value = result
    } else {
      errorMsg.value = '尚未生成分析结果，请点击"重新分析"按钮。'
    }
  } catch (e: any) {
    errorMsg.value = String(e?.message || e?.toString() || '未知错误')
    console.error('[文件夹分析] loadAnalysis 失败:', e)
  } finally {
    loading.value = false
  }
}

const applyAnalysis = (data: DownloadsAnalysis) => {
  analysis.value = data
  loading.value = false
  errorMsg.value = ''
}

const reanalyze = async () => {
  loading.value = true
  errorMsg.value = ''
  try {
    await invoke('analyze_downloads_cmd', { customPath: customPath.value })
    await invoke('show_downloads_analyze_window')
  } catch (e: any) {
    errorMsg.value = String(e?.message || e?.toString() || '未知错误')
    loading.value = false
  }
}

const closeWindow = async () => {
  await invoke('close_downloads_analyze').catch(() => {})
}

const startOrganize = async () => {
  organizing.value = true
  errorMsg.value = ''
  try {
    const conflicts = await invoke<Array<{fileName: string, sourcePath: string, targetFolder: string, targetPath: string}>>('check_downloads_conflicts_cmd', { customPath: customPath.value })

    let strategy: 'Overwrite' | 'Rename' | 'Skip' = 'Rename'
    if (conflicts.length > 0) {
      conflictList.value = conflicts
      const choice = await new Promise<'Overwrite' | 'Rename' | 'Skip' | 'Cancel'>((resolve) => {
        conflictResolver = resolve
        showConflictDialog.value = true
      })
      showConflictDialog.value = false
      conflictResolver = null
      if (choice === 'Cancel') {
        organizing.value = false
        return
      }
      strategy = choice
    }

    const result = await invoke<[number, number, number, number, string[]]>('organize_downloads_cmd', {
      strategy,
      customPath: customPath.value,
    })
    const [exeCount, imageCount, archiveCount, otherCount, errors] = result
    alert(
      `整理完成！\n` +
        `可执行文件: ${exeCount} 个\n` +
        `图片文件: ${imageCount} 个\n` +
        `压缩包: ${archiveCount} 个\n` +
        `其他文件/文件夹: ${otherCount} 个\n` +
        (errors.length > 0 ? `\n错误: ${errors.length} 个\n${errors.slice(0, 5).join('\n')}` : '')
    )
    await invoke('analyze_downloads_cmd', { customPath: customPath.value })
    await invoke('show_downloads_analyze_window')
  } catch (e: any) {
    errorMsg.value = String(e?.message || e?.toString() || '未知错误')
  } finally {
    organizing.value = false
  }
}

const getCategoryLabel = (key: string) => categoryMeta[key]?.label ?? key
const getCategoryColor = (key: string) => categoryMeta[key]?.color ?? '#374151'
const getCategoryBg = (key: string) => categoryMeta[key]?.bg ?? '#F3F4F6'

const copyToClipboard = async (text: string) => {
  try {
    await navigator.clipboard.writeText(text)
  } catch {}
}

const resolveConflict = (strategy: 'Overwrite' | 'Rename' | 'Skip' | 'Cancel') => {
  if (conflictResolver) {
    conflictResolver(strategy)
  }
}

onMounted(async () => {
  unlisteners.push(await listen<DownloadsAnalysis>('downloads-analyze-reload', (event) => {
    console.log('[文件夹分析] 收到 downloads-analyze-reload 事件')
    if (event.payload) {
      applyAnalysis(event.payload)
    }
  }))

  await loadAnalysis()
})

onUnmounted(() => {
  unlisteners.forEach(fn => fn())
})
</script>

<template>
  <div class="analyze-root">
    <div class="header">
      <div class="title-area">
        <h1>文件夹分析</h1>
        <div class="actions">
          <button class="btn btn-primary" :disabled="loading || organizing" @click="startOrganize">
            {{ organizing ? '整理中...' : '开始整理' }}
          </button>
          <button class="btn btn-default" :disabled="organizing" @click="reanalyze">
            {{ loading ? '分析中...' : '重新分析' }}
          </button>
          <button class="btn btn-default" @click="closeWindow">关闭</button>
        </div>
      </div>
      <div v-if="analysis" class="paths">
        <div class="path-row">
          <strong>分析目录：</strong>
          <code>{{ analysis.downloadsPath }}</code>
        </div>
        <button class="btn btn-sm btn-default path-btn" @click="selectFolder">选择文件夹</button>
    </div>
    </div>

    <div v-if="errorMsg" class="error-banner">
      {{ errorMsg }}
    </div>

    <div v-if="analysis" class="stats-grid">
      <div
        v-for="stat in statsList"
        :key="stat.key"
        class="stat-card"
        :class="{ active: activeCategory === stat.key }"
        :style="{ background: stat.bg, borderColor: stat.color }"
        @click="activeCategory = stat.key"
      >
        <div class="stat-count" :style="{ color: stat.color }">{{ stat.count }}</div>
        <div class="stat-label">{{ stat.label }}</div>
      </div>
    </div>

    <div v-if="analysis" class="list-area">
      <div class="list-header">
        <span>显示 {{ filteredItems.length }} 项 (共 {{ totalCount }} 项)</span>
      </div>
      <div class="list-content">
        <table class="item-table">
          <thead>
            <tr>
              <th class="col-name">名称</th>
              <th class="col-cat">分类</th>
              <th class="col-type">类型</th>
              <th class="col-path">路径</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(item, idx) in filteredItems" :key="idx">
              <td class="col-name" :title="item.name">{{ item.name }}</td>
              <td class="col-cat">
                <span
                  class="badge"
                  :style="{ background: getCategoryBg(item.category), color: getCategoryColor(item.category) }"
                >
                  {{ getCategoryLabel(item.category) }}
                </span>
              </td>
              <td class="col-type">{{ item.isDir ? '文件夹' : '文件' }}</td>
              <td class="col-path">
                <div v-if="item.path" class="path-line" @click="copyToClipboard(item.path!)">
                  <span class="path-label">路径:</span>
                  <code :title="item.path">{{ item.path }}</code>
                </div>
                <span v-if="!item.path" class="muted">—</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-if="analysis.errors.length > 0" class="errors-section">
        <div class="errors-title">分析过程中的错误 ({{ analysis.errors.length }})：</div>
        <ul>
          <li v-for="(err, i) in analysis.errors" :key="i">{{ err }}</li>
        </ul>
      </div>
    </div>

    <div v-else-if="loading" class="empty-state">正在分析下载目录...</div>
    <div v-else class="empty-state">点击"重新分析"按钮开始分析</div>

    <div v-if="showConflictDialog" class="conflict-dialog-overlay">
      <div class="conflict-dialog">
        <div class="conflict-dialog-title">发现 {{ conflictList.length }} 个同名文件冲突</div>
        <div class="conflict-dialog-body">
          <div class="conflict-list">
            <div v-for="(item, idx) in conflictList.slice(0, 10)" :key="idx" class="conflict-item">
              <span class="conflict-name">{{ item.fileName }}</span>
              <span class="conflict-arrow">→</span>
              <span class="conflict-target">{{ item.targetFolder }}</span>
            </div>
            <div v-if="conflictList.length > 10" class="conflict-more">
              ...等共 {{ conflictList.length }} 个冲突文件
            </div>
          </div>
        </div>
        <div class="conflict-dialog-actions">
          <button class="conflict-btn conflict-btn-danger" @click="resolveConflict('Overwrite')">
            覆盖同名文件
          </button>
          <button class="conflict-btn conflict-btn-primary" @click="resolveConflict('Rename')">
            自动改名
          </button>
          <button class="conflict-btn conflict-btn-default" @click="resolveConflict('Skip')">
            不移动重名文件
          </button>
          <button class="conflict-btn conflict-btn-cancel" @click="resolveConflict('Cancel')">
            取消整理
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.analyze-root {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  background: #f5f5f5;
  font-size: 13px;
  overflow: hidden;
}

.header {
  background: #fff;
  padding: 12px 16px;
  border-bottom: 1px solid #e5e7eb;
}

.title-area {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.title-area h1 {
  font-size: 16px;
  font-weight: 600;
  margin: 0;
  color: #111827;
}

.actions {
  display: flex;
  gap: 8px;
}

.btn {
  padding: 6px 14px;
  border: 1px solid #d1d5db;
  background: #fff;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.btn:hover {
  background: #f9fafb;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-primary {
  background: #2563eb;
  color: #fff;
  border-color: #2563eb;
}

.btn-primary:hover {
  background: #1d4ed8;
}

.btn-sm {
  padding: 4px 10px;
  font-size: 11px;
}

.paths {
  margin-top: 8px;
  font-size: 11px;
  color: #6b7280;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.path-row {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 1;
  min-width: 0;
}

.path-btn {
  flex-shrink: 0;
}

.paths code {
  background: #f3f4f6;
  padding: 1px 4px;
  border-radius: 2px;
  font-size: 11px;
  color: #374151;
}

.error-banner {
  background: #fef2f2;
  color: #991b1b;
  padding: 8px 16px;
  font-size: 12px;
  border-bottom: 1px solid #fecaca;
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 8px;
  padding: 12px 16px;
  background: #fff;
  border-bottom: 1px solid #e5e7eb;
}

.stat-card {
  padding: 10px 8px;
  border: 2px solid transparent;
  border-radius: 6px;
  cursor: pointer;
  text-align: center;
  transition: transform 0.15s, box-shadow 0.15s;
}

.stat-card:hover {
  transform: translateY(-1px);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.08);
}

.stat-card.active {
  border-color: currentColor;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
}

.stat-count {
  font-size: 22px;
  font-weight: 700;
  line-height: 1.1;
}

.stat-label {
  font-size: 11px;
  color: #4b5563;
  margin-top: 2px;
}

.list-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: #fff;
}

.list-header {
  padding: 6px 16px;
  font-size: 11px;
  color: #6b7280;
  border-bottom: 1px solid #f3f4f6;
}

.list-content {
  flex: 1;
  overflow: auto;
}

.item-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

.item-table th {
  text-align: left;
  padding: 6px 8px;
  background: #f9fafb;
  border-bottom: 1px solid #e5e7eb;
  font-weight: 600;
  color: #374151;
  font-size: 11px;
  position: sticky;
  top: 0;
  z-index: 1;
}

.item-table td {
  padding: 5px 8px;
  border-bottom: 1px solid #f3f4f6;
  vertical-align: top;
  color: #374151;
}

.item-table tr:hover td {
  background: #f9fafb;
}

.col-name {
  max-width: 180px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  word-break: break-all;
}

.col-cat {
  width: 120px;
}

.col-type {
  width: 60px;
  color: #6b7280;
}

.col-path {
  max-width: 320px;
}

.badge {
  display: inline-block;
  padding: 2px 6px;
  border-radius: 3px;
  font-size: 10px;
  white-space: nowrap;
}

.path-line {
  display: flex;
  gap: 4px;
  align-items: baseline;
  cursor: pointer;
  font-size: 11px;
  margin-bottom: 2px;
}

.path-line:hover code {
  background: #e5e7eb;
}

.path-label {
  color: #9ca3af;
  flex-shrink: 0;
}

.path-line code {
  background: #f3f4f6;
  padding: 1px 4px;
  border-radius: 2px;
  font-size: 11px;
  color: #374151;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 260px;
  display: inline-block;
}

.muted {
  color: #9ca3af;
}

.errors-section {
  padding: 8px 16px;
  background: #fef2f2;
  border-top: 1px solid #fecaca;
  font-size: 11px;
  color: #991b1b;
  max-height: 120px;
  overflow: auto;
}

.errors-title {
  font-weight: 600;
  margin-bottom: 4px;
}

.errors-section ul {
  margin: 0;
  padding-left: 18px;
}

.empty-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #9ca3af;
  font-size: 14px;
}

.conflict-dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.conflict-dialog {
  background: #fff;
  border-radius: 8px;
  width: 420px;
  max-width: 90%;
  box-shadow: 0 10px 25px rgba(0, 0, 0, 0.2);
  overflow: hidden;
}

.conflict-dialog-title {
  padding: 14px 16px;
  font-size: 15px;
  font-weight: 600;
  color: #111827;
  background: #f9fafb;
  border-bottom: 1px solid #e5e7eb;
}

.conflict-dialog-body {
  padding: 12px 16px;
  max-height: 200px;
  overflow-y: auto;
}

.conflict-list {
  font-size: 12px;
}

.conflict-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
  color: #374151;
}

.conflict-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.conflict-arrow {
  color: #9ca3af;
  flex-shrink: 0;
}

.conflict-target {
  color: #6b7280;
  flex-shrink: 0;
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.conflict-more {
  padding: 4px 0;
  color: #6b7280;
  font-size: 11px;
}

.conflict-dialog-actions {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 16px;
  border-top: 1px solid #e5e7eb;
  background: #f9fafb;
}

.conflict-btn {
  padding: 8px 16px;
  border: 1px solid #d1d5db;
  background: #fff;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
  text-align: center;
}

.conflict-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

.conflict-btn-danger {
  background: #dc2626;
  color: #fff;
  border-color: #dc2626;
}

.conflict-btn-danger:hover {
  background: #b91c1c;
}

.conflict-btn-primary {
  background: #2563eb;
  color: #fff;
  border-color: #2563eb;
}

.conflict-btn-primary:hover {
  background: #1d4ed8;
}

.conflict-btn-default {
  background: #fff;
  color: #374151;
  border-color: #d1d5db;
}

.conflict-btn-default:hover {
  background: #f9fafb;
}

.conflict-btn-cancel {
  background: #6b7280;
  color: #fff;
  border-color: #6b7280;
}

.conflict-btn-cancel:hover {
  background: #4b5563;
}
</style>