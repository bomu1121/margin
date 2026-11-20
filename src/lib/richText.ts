import DOMPurify from 'dompurify'
import { marked } from 'marked'

import { renderVisualBlock } from './visualBlock'

/**
 * GFM emphasis needs flanking rules that full-width CJK punctuation breaks:
 * `以**会话（session）**为单位` is not parsed as bold by marked, so the
 * asterisks leak into the UI. Convert `**…**` to inline HTML outside fenced
 * blocks and inline code first; marked passes the tag through, DOMPurify
 * keeps it safe.
 */
function emphasizeCjk(text: string): string {
  return text.replace(/\*\*([^*\n]+?)\*\*/g, '<strong>$1</strong>')
}

function emphasizeMarkdown(value: string): string {
  let inFence = false
  return value
    .split('\n')
    .map((line) => {
      if (/^\s*```/.test(line)) {
        inFence = !inFence
        return line
      }
      if (inFence) return line
      return line
        .split(/(`[^`\n]*`)/g)
        .map((part, index) => (index % 2 === 1 ? part : emphasizeCjk(part)))
        .join('')
    })
    .join('\n')
}

interface VisualSegment {
  type: 'text' | 'html'
  text: string
}

/** Split ```html fences out so the iframe is inserted after sanitization. */
function splitVisualFences(value: string): VisualSegment[] {
  const lines = value.split('\n')
  const parts: VisualSegment[] = []
  let pending: string[] = []
  const flush = () => {
    if (!pending.length) return
    parts.push({ type: 'text', text: pending.join('\n') })
    pending = []
  }
  for (let index = 0; index < lines.length; index += 1) {
    if (!/^\s*```html\s*$/.test(lines[index])) {
      pending.push(lines[index])
      continue
    }
    let end = index + 1
    const body: string[] = []
    while (end < lines.length && !/^\s*```\s*$/.test(lines[end])) {
      body.push(lines[end])
      end += 1
    }
    if (end >= lines.length) {
      pending.push(lines[index])
      continue
    }
    flush()
    parts.push({ type: 'html', text: body.join('\n') })
    index = end
  }
  flush()
  return parts
}

function renderText(value: string): string {
  if (!value.trim()) return ''
  try {
    return DOMPurify.sanitize(marked.parse(emphasizeMarkdown(value), { async: false }) as string, {
      USE_PROFILES: { html: true, svg: true, svgFilters: true },
      FORBID_TAGS: ['script', 'foreignObject'],
      FORBID_ATTR: ['onload', 'onerror'],
    })
  } catch {
    return DOMPurify.sanitize(value)
  }
}

export function renderRichText(value: string): string {
  return splitVisualFences(value)
    .map((part) => (part.type === 'html' ? renderVisualBlock(part.text) : renderText(part.text)))
    .join('')
}
