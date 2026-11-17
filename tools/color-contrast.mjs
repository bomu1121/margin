// WCAG 2.1 contrast audit for the current palette and the four research directions.
// Usage: node tools/color-contrast.mjs
const palettes = {
  current: {
    light: { canvas: '#f4f1e8', surface: '#fffdf8', rail: '#f4f1e8', ink: '#1b1813', ink2: '#514a3f', ink3: '#686055', accent: '#1f4fd8', accentSoft: '#eef2ff', attention: '#8a5d0c', danger: '#b4232f', success: '#187a4d' },
    dark: { canvas: '#0c0b09', surface: '#211d16', rail: '#1a1712', ink: '#eee9df', ink2: '#b7afa2', ink3: '#9a9080', accent: '#7aa2ff', accentSoft: '#1e2738', attention: '#e3ad4f', danger: '#ef7d88', success: '#5cc98e' },
  },
  neutralTool: {
    light: { canvas: '#f6f8fa', surface: '#ffffff', rail: '#eef1f4', ink: '#1f2328', ink2: '#57606a', ink3: '#5f6b76', accent: '#0969da', accentSoft: '#ddf4ff', attention: '#9a6700', danger: '#cf222e', success: '#1a7f37' },
    dark: { canvas: '#0d1117', surface: '#161b22', rail: '#10151c', ink: '#e6edf3', ink2: '#b1bac4', ink3: '#8b949e', accent: '#4493f8', accentSoft: '#12283f', attention: '#d29922', danger: '#f85149', success: '#3fb950' },
  },
  warmEditorial: {
    light: { canvas: '#f7f7f5', surface: '#ffffff', rail: '#f1f1ee', ink: '#2f2f2b', ink2: '#5f5e58', ink3: '#6b6a63', accent: '#1668c4', accentSoft: '#f1f7ff', attention: '#9a6700', danger: '#c03936', success: '#0f7b6c' },
    dark: { canvas: '#191919', surface: '#202020', rail: '#1c1c1c', ink: '#e9e9e6', ink2: '#b8b8b3', ink3: '#93938c', accent: '#5aa7f0', accentSoft: '#1b2b3a', attention: '#d6a43a', danger: '#ef7d88', success: '#5cc98e' },
  },
  graphiteIndigo: {
    light: { canvas: '#f7f8fa', surface: '#ffffff', rail: '#eef0f4', ink: '#16181d', ink2: '#4b5058', ink3: '#626a76', accent: '#4f5bc4', accentSoft: '#eef0ff', attention: '#9a6700', danger: '#d14343', success: '#1a7f5a' },
    dark: { canvas: '#08090a', surface: '#101113', rail: '#0d0e10', ink: '#f7f8f8', ink2: '#c9ced6', ink3: '#8a8f98', accent: '#7170ff', accentSoft: '#15162c', attention: '#e2a336', danger: '#f34e52', success: '#27a644' },
  },
  warmNoBlue: {
    light: { canvas: '#f7f6f3', surface: '#ffffff', rail: '#f1efeb', ink: '#2e2c29', ink2: '#5c5953', ink3: '#6f6b64', accent: '#b4432f', accentSoft: '#f9ece8', attention: '#8a5a00', danger: '#a61b2b', success: '#2f7d5d' },
    dark: { canvas: '#171614', surface: '#201e1b', rail: '#1a1917', ink: '#eee9e1', ink2: '#bdb6aa', ink3: '#91897c', accent: '#e07a5f', accentSoft: '#3a2621', attention: '#d6a43a', danger: '#f07a7a', success: '#5cc98e' },
  },
}

function channel(value) {
  const v = value / 255
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4
}
function luminance(hex) {
  const value = hex.replace('#', '')
  const n = value.length === 3 ? value.split('').map((c) => c + c).join('') : value
  return 0.2126 * channel(parseInt(n.slice(0, 2), 16)) + 0.7152 * channel(parseInt(n.slice(2, 4), 16)) + 0.0722 * channel(parseInt(n.slice(4, 6), 16))
}
function ratio(fg, bg) {
  const a = luminance(fg)
  const b = luminance(bg)
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
}
function pct(v) {
  return Number(v.toFixed(2))
}

const pairs = [
  ['ink on surface', 'ink', 'surface'],
  ['ink on canvas', 'ink', 'canvas'],
  ['ink2 on surface', 'ink2', 'surface'],
  ['ink3 on surface', 'ink3', 'surface'],
  ['ink3 on rail', 'ink3', 'rail'],
  ['accent on surface', 'accent', 'surface'],
  ['accent on accentSoft', 'accent', 'accentSoft'],
  ['attention on surface', 'attention', 'surface'],
  ['danger on surface', 'danger', 'surface'],
  ['success on surface', 'success', 'surface'],
]

const out = {}
for (const [name, modes] of Object.entries(palettes)) {
  out[name] = {}
  for (const [mode, colors] of Object.entries(modes)) {
    out[name][mode] = Object.fromEntries(pairs.map(([label, fg, bg]) => [label, pct(ratio(colors[fg], colors[bg]))]))
  }
}
console.log(JSON.stringify(out, null, 2))
