import type { Component } from 'vue'
import {
  BookOpen,
  Command,
  Cpu,
  Database,
  Info,
  Palette,
  PenLine,
  SlidersHorizontal,
} from 'lucide-vue-next'

export type SettingsSectionKind = 'ready' | 'planned' | 'external'

export interface SettingsSection {
  id: string
  label: string
  description: string
  keywords: string[]
  icon: Component
  kind: SettingsSectionKind
}

/**
 * Settings are declared here once. When a new section lands, add it to this
 * registry and render its panel in SettingsView; navigation/search/deep links
 * then come for free.
 */
export const SETTINGS_SECTIONS: SettingsSection[] = [
  {
    id: 'appearance',
    label: '外观与主题',
    description: '主题、明暗模式与系统跟随。',
    keywords: ['主题', '配色', '颜色', '浅色', '深色', '系统', '外观', 'theme'],
    icon: Palette,
    kind: 'ready',
  },
  {
    id: 'reading',
    label: '阅读与字体',
    description: '正文宽度、字号、行距与字体。',
    keywords: ['阅读', '字体', '字号', '行距', '正文', '宽度'],
    icon: BookOpen,
    kind: 'planned',
  },
  {
    id: 'editor',
    label: '编辑与标注',
    description: '保存行为、标注颜色与选区交互。',
    keywords: ['编辑', '标注', '保存', '颜色', '选区'],
    icon: PenLine,
    kind: 'planned',
  },
  {
    id: 'production',
    label: '制作与模型',
    description: 'AI 模型、语音识别、检索、订阅与同步。',
    keywords: ['制作', '模型', 'AI', '语音', '检索', '订阅', '同步'],
    icon: Cpu,
    kind: 'external',
  },
  {
    id: 'data',
    label: '数据与备份',
    description: '笔记库、索引、导入导出与恢复。',
    keywords: ['数据', '备份', '索引', '导入', '导出', '恢复', '笔记库'],
    icon: Database,
    kind: 'planned',
  },
  {
    id: 'shortcuts',
    label: '快捷键',
    description: '全局快捷键与键盘导航。',
    keywords: ['快捷键', '键盘', 'keyboard', 'shortcut'],
    icon: Command,
    kind: 'planned',
  },
  {
    id: 'advanced',
    label: '高级',
    description: '实验特性、日志与诊断。',
    keywords: ['高级', '实验', '日志', '诊断', '开发者'],
    icon: SlidersHorizontal,
    kind: 'planned',
  },
  {
    id: 'about',
    label: '关于',
    description: '版本、本地数据与项目说明。',
    keywords: ['关于', '版本', '说明', '本地'],
    icon: Info,
    kind: 'ready',
  },
]

export function sectionById(id: string | undefined): SettingsSection | undefined {
  return SETTINGS_SECTIONS.find((section) => section.id === id)
}
