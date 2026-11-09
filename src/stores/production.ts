import { computed, reactive } from 'vue'

import * as api from '../lib/api'
import type {
  BiliAccount,
  ProductionJob,
  ProductionPrompt,
  ProductionSettings,
  ProductionStartRequest,
  ProductionTemplate,
} from '../types'

export const production = reactive({
  jobs: [] as ProductionJob[],
  templates: [] as ProductionTemplate[],
  prompts: [] as ProductionPrompt[],
  settings: null as ProductionSettings | null,
  account: null as BiliAccount | null,
  loading: false,
  error: '',
  loaded: false,
})

export const productionUi = reactive({
  view: 'home' as 'home' | 'tasks' | 'templates' | 'prompts' | 'subscriptions' | 'settings',
  wizardOpen: false,
  wizardSource: '',
  wizardTemplate: '',
  taskId: '',
  loginOpen: false,
  settingsTab: 'ai',
})

let unlisten: (() => void) | null = null
let loadingPromise: Promise<void> | null = null

export async function loadProduction(): Promise<void> {
  if (loadingPromise) return loadingPromise
  production.loading = true
  production.error = ''
  loadingPromise = (async () => {
    try {
      const [templates, jobs, settings, prompts, account] = await Promise.all([
        api.productionTemplates(),
        api.productionList(),
        api.productionSettings(),
        api.productionPrompts(),
        api.productionBiliAccount(),
      ])
      production.templates = templates
      production.jobs = jobs
      production.settings = settings
      production.prompts = prompts
      production.account = account
      production.loaded = true
    } catch (error) {
      production.error = api.errorText(error)
    } finally {
      production.loading = false
      loadingPromise = null
    }
  })()
  return loadingPromise
}

export async function refreshJobs(): Promise<void> {
  try {
    production.jobs = await api.productionList()
  } catch (error) {
    production.error = api.errorText(error)
  }
}

export async function refreshSettings(): Promise<void> {
  try {
    production.settings = await api.productionSettings()
    production.account = await api.productionBiliAccount()
  } catch (error) {
    production.error = api.errorText(error)
  }
}

export async function refreshPrompts(): Promise<void> {
  try {
    production.prompts = await api.productionPrompts()
  } catch (error) {
    production.error = api.errorText(error)
  }
}

export function upsertJob(job: ProductionJob): void {
  const index = production.jobs.findIndex((item) => item.id === job.id)
  if (index >= 0) production.jobs.splice(index, 1, job)
  else production.jobs.unshift(job)
  production.jobs.sort((a, b) => b.createdAt - a.createdAt)
}

export async function startProduction(request: Parameters<typeof api.productionStart>[0]) {
  const job = await api.productionStart(request)
  upsertJob(job)
  productionUi.wizardOpen = false
  productionUi.taskId = job.id
  return job
}

export async function startProductionBatch(requests: ProductionStartRequest[]) {
  const jobs: ProductionJob[] = []
  for (const request of requests) {
    const job = await api.productionStart(request)
    upsertJob(job)
    jobs.push(job)
  }
  productionUi.wizardOpen = false
  productionUi.taskId = jobs[0]?.id ?? ''
  return jobs
}

export async function cancelProduction(id: string): Promise<void> {
  await api.productionCancel(id)
  await refreshJobs()
}

export async function retryProduction(id: string) {
  const job = await api.productionRetry(id)
  upsertJob(job)
  return job
}

export async function resumeProduction(id: string) {
  const job = await api.productionResume(id)
  upsertJob(job)
  return job
}

export async function deleteProduction(id: string): Promise<void> {
  await api.productionDelete(id)
  production.jobs = production.jobs.filter((item) => item.id !== id)
  if (productionUi.taskId === id) productionUi.taskId = ''
}

export function initProductionUpdates(): void {
  if (unlisten) return
  void api.onProductionUpdate((job) => upsertJob(job)).then((stop) => {
    unlisten = stop
  })
}

export function disposeProductionUpdates(): void {
  unlisten?.()
  unlisten = null
}

export const activeJobs = computed(() =>
  production.jobs.filter((job) => job.status === 'running' || job.status === 'queued'),
)
export const finishedJobs = computed(() =>
  production.jobs.filter((job) => job.status === 'success'),
)
export const selectedJob = computed(() =>
  production.jobs.find((job) => job.id === productionUi.taskId) ?? null,
)

export function templateById(id: string): ProductionTemplate | undefined {
  return production.templates.find((item) => item.id === id)
}

export function promptNames(): string[] {
  return production.prompts.map((item) => item.name)
}
