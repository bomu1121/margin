import DOMPurify from 'dompurify'
import { marked } from 'marked'

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

export function renderRichText(value: string): string {
  let inFence = false
  const prepared = value
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
  try {
    return DOMPurify.sanitize(marked.parse(prepared, { async: false }) as string)
  } catch {
    return DOMPurify.sanitize(value)
  }
}
