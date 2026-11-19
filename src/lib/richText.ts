import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { renderFigureBlock } from './markdown'

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

/** Replace ```figure fences with the same HTML the note reader renders. */
function replaceFigureFences(text: string): string {
  const lines = text.split('\n')
  const out: string[] = []
  for (let i = 0; i < lines.length; i += 1) {
    if (!/^\s*```figure\s*$/.test(lines[i])) {
      out.push(lines[i])
      continue
    }
    let end = i + 1
    const body: string[] = []
    while (end < lines.length && !/^\s*```\s*$/.test(lines[end])) {
      body.push(lines[end])
      end += 1
    }
    if (end >= lines.length) {
      out.push(lines[i])
      continue
    }
    const { html } = renderFigureBlock(body.join('\n'))
    if (html) out.push('', html, '')
    i = end
  }
  return out.join('\n')
}

export function renderRichText(value: string): string {
  let inFence = false
  const emphasized = value
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
  const prepared = replaceFigureFences(emphasized)
  try {
    return DOMPurify.sanitize(marked.parse(prepared, { async: false }) as string, {
      USE_PROFILES: { html: true, svg: true, svgFilters: true },
      FORBID_TAGS: ['script', 'foreignObject'],
      FORBID_ATTR: ['onload', 'onerror'],
    })
  } catch {
    return DOMPurify.sanitize(value)
  }
}
