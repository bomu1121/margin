export type ThemeId = 'neutral' | 'warm' | 'graphite' | 'noblue'
export type ThemeMode = 'system' | 'light' | 'dark'
export type ResolvedMode = 'light' | 'dark'

export interface ThemePreview {
  bg: string
  surface: string
  rail: string
  ink: string
  accent: string
  warning: string
  line: string
}

export interface ThemeDefinition {
  id: ThemeId
  label: string
  tagline: string
  references: string
  preview: Record<ResolvedMode, ThemePreview>
}

export const THEMES: ThemeDefinition[] = [
  {
    id: 'neutral',
    label: '中性工具',
    tagline: '冷灰白面 + 单一钴蓝；最克制，也最像现代工具。',
    references: 'GitHub Primer / Ant Design',
    preview: {
      light: { bg: '#f6f8fa', surface: '#ffffff', rail: '#eef1f4', ink: '#1f2328', accent: '#0969da', warning: '#9a6700', line: '#d8dee4' },
      dark: { bg: '#0d1117', surface: '#161b22', rail: '#10151c', ink: '#e6edf3', accent: '#4493f8', warning: '#d29922', line: '#30363d' },
    },
  },
  {
    id: 'warm',
    label: '暖灰编辑',
    tagline: '低彩度暖灰 + 深蓝；保留纸感，但不再用米黄和金色。',
    references: 'Notion / Craft',
    preview: {
      light: { bg: '#f7f7f5', surface: '#ffffff', rail: '#f1f1ee', ink: '#2f2f2b', accent: '#1668c4', warning: '#9a6700', line: '#e6e5e0' },
      dark: { bg: '#191919', surface: '#202020', rail: '#1c1c1c', ink: '#e9e9e6', accent: '#5aa7f0', warning: '#d6a43a', line: '#2f2f2e' },
    },
  },
  {
    id: 'graphite',
    label: '石墨靛蓝',
    tagline: '冷灰 + 靛蓝；暗色优先，产品识别更强。',
    references: 'Linear / Stripe',
    preview: {
      light: { bg: '#f7f8fa', surface: '#ffffff', rail: '#eef0f4', ink: '#16181d', accent: '#4f5bc4', warning: '#9a6700', line: '#e3e6eb' },
      dark: { bg: '#08090a', surface: '#101113', rail: '#0d0e10', ink: '#f7f8f8', accent: '#7170ff', warning: '#e2a336', line: '#23252a' },
    },
  },
  {
    id: 'noblue',
    label: '暖灰陶土',
    tagline: '暖灰 + 陶土红；完全没有蓝色，适合实验。',
    references: 'Bear / Craft',
    preview: {
      light: { bg: '#f7f6f3', surface: '#ffffff', rail: '#f1efeb', ink: '#2e2c29', accent: '#b4432f', warning: '#8a5a00', line: '#e6e3dd' },
      dark: { bg: '#171614', surface: '#201e1b', rail: '#1a1917', ink: '#eee9e1', accent: '#e07a5f', warning: '#d6a43a', line: '#2e2b27' },
    },
  },
]

export const DEFAULT_THEME: ThemeId = 'neutral'
export const DEFAULT_MODE: ThemeMode = 'system'

const PALETTE_KEY = 'yebian.theme.palette'
const MODE_KEY = 'yebian.theme.mode'
const THEME_EVENT = 'yb-theme-change'

function isThemeId(value: string | null): value is ThemeId {
  return value === 'neutral' || value === 'warm' || value === 'graphite' || value === 'noblue'
}

function isThemeMode(value: string | null): value is ThemeMode {
  return value === 'system' || value === 'light' || value === 'dark'
}

export function readThemePreference(): { theme: ThemeId; mode: ThemeMode } {
  try {
    const theme = window.localStorage.getItem(PALETTE_KEY)
    const mode = window.localStorage.getItem(MODE_KEY)
    return {
      theme: isThemeId(theme) ? theme : DEFAULT_THEME,
      mode: isThemeMode(mode) ? mode : DEFAULT_MODE,
    }
  } catch {
    return { theme: DEFAULT_THEME, mode: DEFAULT_MODE }
  }
}

export function systemMode(): ResolvedMode {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export function resolveMode(mode: ThemeMode): ResolvedMode {
  return mode === 'system' ? systemMode() : mode
}

export function writeThemePreference(theme: ThemeId, mode: ThemeMode): void {
  try {
    window.localStorage.setItem(PALETTE_KEY, theme)
    window.localStorage.setItem(MODE_KEY, mode)
  } catch {
    /* A locked-down webview can still use the theme for this session. */
  }
}

export function applyTheme(theme: ThemeId, mode: ThemeMode): ResolvedMode {
  const resolved = resolveMode(mode)
  const root = document.documentElement
  root.dataset.theme = theme
  root.dataset.mode = resolved
  root.style.colorScheme = resolved
  window.dispatchEvent(new CustomEvent(THEME_EVENT, { detail: { theme, mode, resolved } }))
  return resolved
}

export function setThemePreference(theme: ThemeId, mode: ThemeMode): ResolvedMode {
  writeThemePreference(theme, mode)
  return applyTheme(theme, mode)
}

export function onThemeChange(listener: () => void): () => void {
  window.addEventListener(THEME_EVENT, listener)
  return () => window.removeEventListener(THEME_EVENT, listener)
}
