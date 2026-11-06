import type {
  BiliVideoRef,
  ProductionJob,
  ProductionSource,
  ProductionTemplate,
} from '../types'

export function formatTokens(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '—'
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`
  return String(Math.round(value))
}

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0 B'
  if (value < 1024) return `${Math.round(value)} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(0)} KB`
  if (value < 1024 * 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`
  return `${(value / 1024 / 1024 / 1024).toFixed(2)} GB`
}

export function formatDuration(seconds: number): string {
  if (!seconds) return '—'
  const minutes = Math.floor(seconds / 60)
  const rest = Math.floor(seconds % 60)
  return `${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`
}

export function formatDate(timestamp: number): string {
  if (!timestamp) return '—'
  const date = new Date(timestamp)
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  const hour = String(date.getHours()).padStart(2, '0')
  const minute = String(date.getMinutes()).padStart(2, '0')
  return `${month}-${day} ${hour}:${minute}`
}

export function statusLabel(status: string): string {
  return ({
    queued: '排队中',
    running: '进行中',
    success: '已完成',
    error: '失败',
    cancelled: '已取消',
    interrupted: '已中断',
  } as Record<string, string>)[status] ?? status
}

export function statusClass(status: string): string {
  if (status === 'success') return 'is-success'
  if (status === 'error') return 'is-error'
  if (status === 'queued' || status === 'running') return 'is-running'
  if (status === 'interrupted') return 'is-gold'
  return 'is-muted'
}

export function sourceLabel(kind: string): string {
  return ({
    text: '粘贴文稿',
    file: '本地音视频',
    bili: 'B站链接',
    favorites: 'B站收藏',
  } as Record<string, string>)[kind] ?? kind
}

export function sourceSummary(source: ProductionSource): string {
  if (source.kind === 'text') return `粘贴文稿 · ${source.text.length} 字`
  if (source.kind === 'file') return `本地文件 · ${source.path.split(/[\\/]/).pop() ?? source.path}`
  if (source.kind === 'bili') return `B站链接 · ${source.url}`
  if (source.kind === 'favorites') {
    const names = source.videos.map((item) => item.title || item.bvid).slice(0, 2)
    return `B站收藏 · ${source.videos.length} 个视频${names.length ? ` · ${names.join('、')}` : ''}`
  }
  return source.kind
}

export function templateById(templates: ProductionTemplate[], id: string): ProductionTemplate | undefined {
  return templates.find((item) => item.id === id)
}

export function jobTemplate(job: ProductionJob, templates: ProductionTemplate[]): ProductionTemplate | undefined {
  return templateById(templates, job.templateId)
}

export function isActive(job: ProductionJob): boolean {
  return job.status === 'running' || job.status === 'queued'
}

export function progressOf(job: ProductionJob): number {
  if (job.status === 'success') return 100
  return Math.max(0, Math.min(99, job.progress))
}

export function biliVideoToRef(video: BiliVideoRef): BiliVideoRef {
  return {
    bvid: video.bvid,
    title: video.title,
    author: video.author,
    duration: video.duration,
    pubdate: video.pubdate,
    cover: video.cover,
    cid: video.cid,
  }
}

/** B站图床常见 //i0.hdslb.com 或 http://；WebView CSP 只放行 https。 */
export function mediaUrl(url: string): string {
  if (!url) return ''
  if (url.startsWith('//')) return `https:${url}`
  if (url.startsWith('http://')) return `https://${url.slice(7)}`
  return url
}
