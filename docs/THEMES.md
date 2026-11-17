# 主题系统与设置页（2026-11-17）

> 四套主题已从 demo 落成应用默认主题：`neutral / warm / graphite / noblue`。全局设置页在 `/settings/appearance`，支持跟随系统、浅色、深色，主题保存在本机。

## 0. 结论

1. 主题不是四组随机色板，而是四套同一角色规则的 token：**中性表面 + 一个主色 + 状态色只用于状态**。金色从“历史分类”里撤掉，只保留给真正的 warning。
2. 模式（`system/light/dark`）和主题（四套）分开。`system` 只是解析成当前浅/深，不覆盖用户的主题选择。
3. 主题在 `index.html` 的阻塞内联脚本里、首屏绘制前写入 `<html>`；CSS 加载后不会再闪一下旧主题。
4. 设置页采用“左侧分类 + 搜索 + 深链接 + 注册表”的结构。新增设置只登记一个 section 并补面板，不再散落到各个页面。
5. 实测：笔记页 8 个主题/模式组合、设置页 8 个组合、事实库 8 个组合，`audit-text` 全部 0 个 <12px、0 个低对比；设置页 1040/600 无横向溢出。

## 1. 调研

### 1.1 成熟产品的主题切换

| 产品 | 做法 | 借用点 |
| --- | --- | --- |
| **VS Code** | `File > Preferences > Color Theme`；上下键**实时预览**；主题写入 `workbench.colorTheme`；可配置 preferred light/dark，开启 `window.autoDetectColorScheme` 跟随系统 | 主题列表要能即时预览；主题和明暗模式分开 |
| **Obsidian** | `Settings → Appearance → Base color`：跟随系统 / 浅色 / 深色；Theme 是另一层，可安装社区主题、调 accent | “基础色”和“主题包”分开；跟随系统是默认 |
| **Linear** | `Settings → Preferences`：Light / Dark / System，另有高对比和自定义主题；主题可按设备和明暗模式保存 | 模式是偏好，不是主题本身；暗色要单独设计 |
| **Raycast** | `Settings → General → Appearance`；默认跟随系统；可以给 Light 和 Dark 分别指定主题；Theme Studio 预览/编辑 | 同一个产品可以有“浅色主题”和“深色主题”两个选择 |
| **GitHub** | `Settings → Appearance`；Light/Dark/System；跟随系统时分别选 day/night；另有高对比、色盲主题 | 可访问性主题是独立选项，不是把默认主题调一调 |

共同结论：

- **模式与主题分离**：用户先决定“跟随系统/浅/深”，再决定“哪套主题”。
- **系统模式是 fallback，不是覆盖**：显式选择必须优先于 OS。
- **预览必须按主题自己的固定色渲染**，不能拿当前主题 token 去渲染其他主题的卡片。
- 选择即时生效；主题属于设备偏好，不需要“保存”按钮。

### 1.2 设置页结构

调研的产品/模式里，设置页稳定采用：

- **左侧垂直分类**，不用横向 tab 堆到十几个；
- **搜索框在分类上方**，结果要能定位到 section，而不是只过滤标题；
- **URL 是状态**，深链接到 `/settings/<section>`，前进/后退可用；
- **单选/开关自动保存**，多字段表单才显式保存；
- **注册表驱动**：section 有 id/label/description/keywords/icon/status，新增设置不再改导航；
- **计划中的 section 明确标“计划”**，不放假开关；
- 移动端把左栏折叠成单列，不保留两栏。

### 1.3 避免主题闪烁

采用业界通行的做法：

1. `<meta name="color-scheme" content="light dark">` 放在 stylesheet 前；
2. 在 `<head>` 里放一个同步内联脚本，读 `localStorage` 和 `prefers-color-scheme`，在首屏绘制前设置 `data-theme` / `data-mode` / `style.colorScheme`；
3. 首屏加 `theme-booting` 类，禁用 transition，DOMContentLoaded 后下一帧移除；
4. 应用运行时监听 `matchMedia('(prefers-color-scheme: dark)')`，只在 `mode === 'system'` 时更新解析后的模式；
5. CSS 只按 `data-mode` 分支，不再按 `@media (prefers-color-scheme)` 分支，否则显式浅色会被 OS 深色覆盖。

## 2. 实现

### 2.1 数据模型

```ts
type ThemeId = 'neutral' | 'warm' | 'graphite' | 'noblue'
type ThemeMode = 'system' | 'light' | 'dark'
```

存储键：

- `yebian.theme.palette`
- `yebian.theme.mode`

默认值：`neutral + system`。主题是设备本地偏好，不写入笔记库、不参与云同步。

关键文件：

- `src/lib/theme.ts`：主题定义、读取/写入、resolve system、应用 `data-*`、主题事件。
- `src/stores/appearance.ts`：Vue reactive 状态、系统媒体监听、reset。
- `index.html`：首屏前同步设置属性。
- `src/styles/themes.css`：四套主题的 token 与组件级中性化覆盖。

### 2.2 四套主题

| 主题 | 参考 | 浅色 | 暗色 |
| --- | --- | --- | --- |
| **A 中性工具** | GitHub / Ant / Vercel | 冷灰白画布 + 白阅读面 + 钴蓝 | 中性炭黑 + 蓝 |
| **B 暖灰编辑** | Notion / Craft | 低彩度暖灰 + 深蓝，无金色 | 暖中性 + 蓝 |
| **C 石墨靛蓝** | Linear / Stripe | 冷灰 + 靛蓝 | 石墨黑 + 靛蓝 |
| **D 暖灰陶土** | Bear / Craft | 暖灰 + 陶土红，无蓝 | 暖炭黑 + 陶土红 |

统一角色：

- `--paper` / `--surface` / `--rail`：三档中性表面；
- `--ink` / `--ink-2` / `--ink-3`：正文/次级/弱化；
- `--blue`：唯一主色（D 是陶土红）；
- `--blue-soft`：**选中态的中性底色**，不是大面积淡蓝；
- `--gold`：warning；不再表示历史；
- `--green` / `--red`：成功/错误；
- `--warning-ink`：warning 文字在浅色纸面上的深色版本。

### 2.3 设置页

路由：`/settings/:section?`，默认 `/settings/appearance`。

- 左栏：搜索 + section 列表，`aria-current="page"`；计划中的 section 有“计划”标记。
- 外观页：明暗模式三选一；四张主题卡；当前主题摘要；恢复默认；设备本地说明。
- 主题卡：固定按主题自己的浅/深色渲染预览，不跟当前主题变色；单选键盘左右/上下切换。
- 制作与模型：不复制表单，直接跳转制作台设置。
- 计划中的 section：明确空态，说明后续内容会从注册表接入。

截图：

- `shots/audit/230-settings-neutral-light-1440.png`
- `shots/audit/231-settings-warm-dark-1440.png`
- `shots/audit/234-settings-neutral-1040.png`
- `shots/audit/235-settings-neutral-600.png`

### 2.4 旧暗色分支的清理

旧 CSS 有 9 处 `@media (prefers-color-scheme: dark)`（token、rail、事实状态、标注、制作警告、史料权威等）。如果保留，用户显式选浅色但 OS 是深色时会得到错误颜色。现在：

- 旧 token 媒体块保留为 fallback，但 `themes.css` 的 `:root[data-theme][data-mode]` 优先级更高；
- 组件级暗色覆盖改为 `:root[data-mode="dark"] ...`；
- 字号/对比度硬编码 `#7a5008` 改为 `var(--warning-ink)`；
- 历史/史料类型使用中性 chip，不再用金色分类。

## 3. 验证

### 3.1 主题矩阵

`audit-text`（1660×1202，浅/深各一次）：

| 页面 | 节点 | <12px | <4.5:1 |
| --- | ---: | ---: | ---: |
| 笔记页（8 个主题/模式组合） | 189 | 0 | 0 |
| 设置页（8 个组合） | 76 | 0 | 0 |
| 事实库（8 个组合） | 382 | 0 | 0 |
| 史料库（neutral light / noblue dark） | 104 | 0 | 0 |
| 史料详情（neutral light / noblue dark） | 91 | 0 | 0 |

### 3.2 行为

- 点击主题卡：`data-theme`、`--paper`、`--blue` 即时变化，`localStorage` 写入正确。
- 点击深色：`data-mode="dark"`，`color-scheme=dark`，旧媒体分支不覆盖。
- 系统模式：`Emulation.setEmulatedMedia` 切换 light/dark，`data-mode` 跟随切换。
- 显式浅色 + 模拟 OS 深色：`data-mode="light"`、`--paper=#f6f8fa`、`color-scheme=light`，没有被旧 `@media` 拉回深色。
- 设置页 1040/600：`scrollWidth === clientWidth`，无横向溢出。
- 最终 `npm run tauri build`：installer `8,797,820 B` / exe `22,126,592 B`；安装版已重启。

### 3.3 截图

- 主题矩阵：`shots/audit/233-note-<theme>-<mode>-1440.png`（8 张）
- 设置页：`230` / `231` / `234` / `235`
- 事实库：`240` / `241`
- 史料库：`242` / `243`
- 图谱：`250`；笔记页局部关系图：`251`

## 4. 诚实清单

- **图谱仍是一个固定深色数据画布**：节点/边/顶栏的语义色已跟随主题，但 `force.css` 里还有少数硬编码的旧光晕 `rgba`，需要下一轮逐个替换；它不会变成浅色图谱。
- 只对笔记页、设置页、事实库、史料库、史料详情做了主题矩阵审计；标签台、制作台、任务详情、图谱没有逐页跑 8 组合的 `audit-text`。
- 只算 WCAG 2.1，没有算 APCA；没有做色盲模拟、真实屏幕校色、用户测试。
- 没有录屏逐帧验证首屏无闪烁；只能确认内联脚本在 module 之前执行、属性和 `color-scheme` 在样式加载前写入。
- 四套是内置主题，不支持用户自定义主题/导入主题；没有高对比/色盲专用主题。
- 设置页的“阅读与字体 / 编辑与标注 / 数据与备份 / 快捷键 / 高级”目前是计划中，不是可用设置。
- 主题是设备本地偏好，不跨设备同步；这是刻意选择，不是遗漏。

## 5. 参考

- [VS Code — Color themes](https://code.visualstudio.com/docs/configure/themes)：实时预览、`workbench.colorTheme`、preferred light/dark、`autoDetectColorScheme`。
- [Obsidian — Change the look](https://help.obsidian.md/Getting+started/Change+the+look)：Base color（跟随系统/浅/深）与 Theme 分层。
- [Linear — Account preferences](https://linear.app/docs/account-preferences)：Light / Dark / System、按设备与明暗模式保存。
- [Raycast — Themes](https://manual.raycast.com/themes)：默认跟随系统、浅/深主题分别指定。
- [GitHub — Managing your theme settings](https://docs.github.com/en/get-started/accessibility/managing-your-theme-settings)：Light/Dark/System、day/night、高对比与色盲主题。
- [Chrome — Dark mode guidance](https://developer.chrome.com/docs/modern-web-guidance/guides/visual-design/dark-mode)：`color-scheme`、根元素主题、避免首屏闪烁。
- [WCAG 2.2 contrast minimum](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html)：本次审计使用的 4.5:1 下限。
