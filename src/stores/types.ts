export interface Task {
  id: number
  text: string
  status: boolean
  color: string
  bold: boolean
  timerType: string
  timerValue: number
  timerRemaining: number
  created_at: string
  orderIndex: number
}

export interface DeletedTask {
  id: number
  originalId: number
  text: string
  status: boolean
  color: string
  bold: boolean
  timerType: string
  timerValue: number
  timerRemaining: number
  created_at: string
  orderIndex: number
  deletedAt: string
}

export interface TimerState {
  task_id: number
  remaining: number
  hours: number
  minutes: number
  seconds: number
  formatted: string
  is_running: boolean
}

export interface ExpiredTask {
  task_id: number
  task_title: string
  timerType: string
  lastTimerValue: number
}

export interface ErrorAlert {
  show: boolean
  title: string
  message: string
}

export interface CategoryResult {
  name: string
  deleted: number
  skipped: number
  freedBytes: number
}

export interface CleanStats {
  scanned: number
  deleted: number
  skipped: number
  freedBytes: number
  currentCategory: string
  currentPath: string
  isRunning: boolean
  errorDetails: string[]
  categories: CategoryResult[]
}