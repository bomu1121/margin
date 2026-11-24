import DOMPurify from 'dompurify'

import { onThemeChange } from './theme'

/**
 * Visual blocks are the successor to the old five figure templates.
 *
 * The model writes a self-contained HTML fragment; the app hosts it in a
 * sandboxed iframe at the note's content width. The app owns the environment
 * (width, theme tokens, no network, no app access) and the model owns the
 * layout. That keeps the note's Markdown prose annotatable/searchable while
 * letting the model design the visual part directly.
 */

const THEME_VARS = [
  '--paper',
  '--surface',
  '--rail',
  '--ink',
  '--ink-2',
  '--ink-3',
  '--line',
  '--line-2',
  '--blue',
  '--blue-soft',
  '--blue-line',
  '--gold',
  '--green',
  '--red',
  '--sans',
  '--serif',
  '--mono',
]

let sequence = 0

function nextId(): string {
  sequence += 1
  return `ybv${Date.now().toString(36)}${sequence.toString(36)}`
}

function escapeAttribute(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

function readThemeVars(): Record<string, string> {
  const styles = getComputedStyle(document.documentElement)
  const vars: Record<string, string> = {}
  for (const name of THEME_VARS) vars[name] = styles.getPropertyValue(name).trim()
  return vars
}

function themeBlock(vars: Record<string, string>): string {
  return `:root{${Object.entries(vars)
    .map(([name, value]) => `${name}:${value}`)
    .join(';')}}`
}

function sanitizeFragment(raw: string): string {
  return DOMPurify.sanitize(raw, {
    USE_PROFILES: { html: true, svg: true, svgFilters: true },
    ADD_TAGS: ['style'],
    FORBID_TAGS: [
      'script',
      'iframe',
      'object',
      'embed',
      'form',
      'input',
      'button',
      'textarea',
      'select',
      'option',
      'link',
      'meta',
      'base',
      'audio',
      'video',
      'source',
      'track',
      'a',
    ],
    FORBID_ATTR: ['srcdoc', 'onload', 'onerror'],
  })
}

/**
 * Models often invent a light palette even when told to use tokens. Neutral
 * greys are the part that breaks dark mode, so map the common ones back to
 * the theme tokens; accent colours are left alone. This is deliberately a
 * value map, not a CSS parser: predictable and easy to audit.
 */
const NEUTRAL_COLORS: Array<[RegExp, string]> = [
  [/#ffffff\b|#fff\b/gi, 'var(--surface)'],
  [/#fffdf8\b|#f7f8fa\b|#f8f9fa\b|#fafafa\b|#f5f5f5\b|#f3f4f6\b|#f9fafb\b/gi, 'var(--paper)'],
  [/#e2e5ea\b|#e5e7eb\b|#e0e0e0\b|#dddddd\b|#ddd\b|#d1d5db\b|#eaedf1\b|#eaeaea\b|#e6e6e6\b/gi, 'var(--line)'],
  [/#1f2430\b|#1f2937\b|#111827\b|#111111\b|#111\b|#222222\b|#222\b|#333333\b|#333\b|#000000\b|#000\b/gi, 'var(--ink)'],
  [/#6b7280\b|#666666\b|#666\b|#777777\b|#777\b|#888888\b|#888\b|#9ca3af\b/gi, 'var(--ink-3)'],
]

function normalizeThemeColors(fragment: string): string {
  const mapped = NEUTRAL_COLORS.reduce((value, [pattern, token]) => value.replace(pattern, token), fragment)
  return mapped.replace(/rgb\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*\)/gi, (match, r, g, b) => {
    const red = Number(r)
    const green = Number(g)
    const blue = Number(b)
    if (Math.max(red, green, blue) - Math.min(red, green, blue) > 12) return match
    const luminance = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255
    if (luminance >= 0.99) return 'var(--surface)'
    if (luminance >= 0.93) return 'var(--paper)'
    if (luminance >= 0.75) return 'var(--line)'
    if (luminance >= 0.3) return 'var(--ink-3)'
    return 'var(--ink)'
  })
}

/** Plain text for FTS, annotations and export. */
export function visualText(raw: string): string {
  return raw
    .replace(/<style[\s\S]*?<\/style>/gi, ' ')
    .replace(/<script[\s\S]*?<\/script>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/\s+/g, ' ')
    .trim()
}

/**
 * Render a model-written HTML fragment as a block-level iframe.
 *
 * The fragment is sanitized first, then embedded in a document we generate.
 * The sandbox allows only its own inline script (height reporting); network,
 * forms, nested frames and app access are blocked by CSP + sandbox. The user
 * never gets to inject a raw iframe: `renderRichText` inserts this string
 * after DOMPurify has already cleaned the Markdown.
 */
export function renderVisualBlock(raw: string): string {
  const fragment = normalizeThemeColors(sanitizeFragment(raw).trim())
  if (!fragment) return ''
  const id = nextId()
  const vars = readThemeVars()
  const doc = `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; script-src 'none'"><style>${themeBlock(vars)}
*,*::before,*::after{box-sizing:border-box}
html,body{margin:0;padding:0;background:transparent;color:var(--ink);font-family:var(--sans);font-size:14px;line-height:1.65;overflow:hidden}
body{padding:2px 0}
img,svg{max-width:100%}
</style></head><body><div class="yb-visual">${fragment}</div></body></html>`
  return `<iframe class="yb-visual-frame" data-yb-visual="${id}" sandbox="allow-same-origin" scrolling="no" loading="eager" title="视觉块" style="width:100%;border:0;display:block;height:240px;background:transparent" srcdoc="${escapeAttribute(doc)}"></iframe>`
}

function measure(frame: HTMLIFrameElement): void {
  try {
    const doc = frame.contentDocument
    if (!doc || !doc.documentElement) return
    const height = Math.max(80, Math.min(4000, Math.ceil(doc.documentElement.scrollHeight)))
    const width = Math.max(doc.documentElement.scrollWidth, doc.body?.scrollWidth || 0)
    const clientWidth = doc.documentElement.clientWidth
    frame.style.height = `${height}px`
    frame.dataset.ybVisualHeight = String(height)
    frame.dataset.ybVisualOverflow = width > clientWidth + 1 ? '1' : '0'
    frame.dataset.ybVisualWidth = `${width}/${clientWidth}`
    if (height >= 4000) frame.dataset.ybVisualClipped = '1'
  } catch {
    // A lazy/remote frame is not measurable; the next scan retries it.
  }
}

function installBridge(): void {
  if (typeof window === 'undefined') return
  const observers = new WeakMap<HTMLIFrameElement, ResizeObserver>()
  const observe = (frame: HTMLIFrameElement): void => {
    measure(frame)
    try {
      const doc = frame.contentDocument
      if (!doc?.documentElement || !doc.body) return
      observers.get(frame)?.disconnect()
      const observer = new ResizeObserver(() => measure(frame))
      observer.observe(doc.documentElement)
      observer.observe(doc.body)
      observers.set(frame, observer)
    } catch {
      // The frame can still be loading; its load event will retry.
    }
  }
  const scan = (): void => {
    document.querySelectorAll<HTMLIFrameElement>('iframe[data-yb-visual]').forEach((frame) => {
      if (frame.dataset.ybVisualBound !== '1') {
        frame.dataset.ybVisualBound = '1'
        frame.addEventListener('load', () => observe(frame), { once: true })
      }
      observe(frame)
    })
  }
  const start = (): void => {
    scan()
    new MutationObserver(scan).observe(document.body, { childList: true, subtree: true })
  }
  if (document.body) start()
  else document.addEventListener('DOMContentLoaded', start, { once: true })

  onThemeChange(() => {
    const vars = readThemeVars()
    document.querySelectorAll<HTMLIFrameElement>('iframe[data-yb-visual]').forEach((frame) => {
      try {
        const root = frame.contentDocument?.documentElement
        if (!root) return
        for (const [name, value] of Object.entries(vars)) root.style.setProperty(name, value)
        measure(frame)
      } catch {
        // The next load/scan will retry.
      }
    })
  })
}

installBridge()
