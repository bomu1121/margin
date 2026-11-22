// Generates the frontend traditional-to-simplified map from the Rust corpus
// dictionary. Run after updating src-tauri/src/production/t2s.txt.
import fs from 'node:fs'
import path from 'node:path'

const source = path.resolve('src-tauri/src/production/t2s.txt')
const target = path.resolve('src/lib/t2s.ts')
const map = {}
for (const line of fs.readFileSync(source, 'utf8').split(/\r?\n/)) {
  const value = line.trim()
  if (!value || value.startsWith('#')) continue
  const [key, rest] = value.split('\t')
  const first = rest?.split(/\s+/)[0]
  if ([...key].length !== 1 || !first || [...first].length !== 1) continue
  map[key] ??= first
}
const body = JSON.stringify(map, null, 0)
fs.writeFileSync(
  target,
  `// Generated from src-tauri/src/production/t2s.txt (OpenCC Apache-2.0).\n` +
    `const MAP: Record<string, string> = ${body}\n\n` +
    `export function toSimplified(value: string): string {\n` +
    `  let output = ''\n` +
    `  for (const char of value || '') output += MAP[char] ?? char\n` +
    `  return output\n` +
    `}\n`,
)
console.log(target, Object.keys(map).length, 'characters')
