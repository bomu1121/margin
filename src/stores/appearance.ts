import { computed, reactive } from 'vue'

import {
  applyTheme,
  DEFAULT_MODE,
  DEFAULT_THEME,
  onThemeChange,
  readThemePreference,
  setThemePreference,
  systemMode,
  type ResolvedMode,
  type ThemeId,
  type ThemeMode,
} from '../lib/theme'

const initial = readThemePreference()

export const appearance = reactive({
  theme: initial.theme,
  mode: initial.mode,
  systemMode: systemMode() as ResolvedMode,
})

export const resolvedMode = computed<ResolvedMode>(() =>
  appearance.mode === 'system' ? appearance.systemMode : appearance.mode,
)

export const isDefaultAppearance = computed(() =>
  appearance.theme === DEFAULT_THEME && appearance.mode === DEFAULT_MODE,
)

function sync(): ResolvedMode {
  const resolved = applyTheme(appearance.theme, appearance.mode)
  appearance.systemMode = systemMode()
  return resolved
}

export function setTheme(theme: ThemeId): void {
  appearance.theme = theme
  setThemePreference(appearance.theme, appearance.mode)
}

export function setThemeMode(mode: ThemeMode): void {
  appearance.mode = mode
  setThemePreference(appearance.theme, appearance.mode)
}

export function resetAppearance(): void {
  appearance.theme = DEFAULT_THEME
  appearance.mode = DEFAULT_MODE
  setThemePreference(appearance.theme, appearance.mode)
}

let media: MediaQueryList | null = null
let disposeThemeListener: (() => void) | null = null

export function initAppearance(): void {
  if (!media) {
    media = window.matchMedia('(prefers-color-scheme: dark)')
    media.addEventListener('change', () => {
      appearance.systemMode = systemMode()
      if (appearance.mode === 'system') sync()
    })
  }
  if (!disposeThemeListener) {
    disposeThemeListener = onThemeChange(() => {
      appearance.systemMode = systemMode()
    })
  }
  // `index.html` already set the attributes before the first paint; this only
  // reconciles the reactive store with the persisted preference.
  sync()
}

export function currentModeLabel(): string {
  if (appearance.mode === 'system') {
    return appearance.systemMode === 'dark' ? '跟随系统 · 当前深色' : '跟随系统 · 当前浅色'
  }
  return appearance.mode === 'dark' ? '深色' : '浅色'
}
