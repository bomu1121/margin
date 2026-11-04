import DOMPurify from 'dompurify'
import { marked } from 'marked'

export interface Block {
  id: string
  text: string
  html: string
  kind: 'p' | 'quote' | 'li' | 'raw'
}

export interface Section {
  heading: string
  blocks: Block[]
}

export interface GlossaryItem {
  cat: string
  term: string
  desc: string
  block: Block
}

export interface ParsedNote {
  lead: Block[]
  summary: Block[]
  sections: Section[]
  glossary: GlossaryItem[]
  allBlocks: Block[]
  generic: boolean
}

marked.setOptions({ gfm: true, breaks: false })

function normalize(text: string): string {
  return text.replace(/\s+/g, ' ').trim()
}

export function blockId(text: string): string {
  const value = normalize(text)
  let hash = 2166136261
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i)
    hash = Math.imul(hash, 16777619)
  }
  return `b${(hash >>> 0).toString(16).padStart(8, '0')}`
}

export function plain(text: string): string {
  return normalize(
    text
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/[*_`>#~]/g, '')
      .replace(/<[^>]+>/g, ''),
  )
}

function sanitize(html: string): string {
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
  })
}

function inline(text: string): string {
  return sanitize(marked.parseInline(text) as string)
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

interface BlockFactory {
  (text: string, kind: Block['kind'], html: string): Block
}

function createFactory(): BlockFactory {
  const seen = new Map<string, number>()
  return (text: string, kind: Block['kind'], html: string) => {
    const base = blockId(text)
    const count = (seen.get(base) ?? 0) + 1
    seen.set(base, count)
    return {
      id: count > 1 ? `${base}-${count}` : base,
      text: plain(text),
      html,
      kind,
    }
  }
}

export function parseNote(markdown: string): ParsedNote {
  const tokens = marked.lexer(markdown) as unknown as Array<Record<string, unknown>>
  const factory = createFactory()

  const lead: Block[] = []
  const summary: Block[] = []
  const sections: Section[] = []
  const glossary: GlossaryItem[] = []
  let current: Section | null = null
  let mode: 'lead' | 'summary' | 'body' | 'glossary' | 'other' = 'lead'
  let sawH2 = false

  const target = (): Section => {
    if (!current) {
      current = { heading: '', blocks: [] }
      sections.push(current)
    }
    return current
  }

  const push = (block: Block) => {
    if (mode === 'summary') {
      summary.push(block)
    } else if (mode === 'body' || mode === 'other') {
      target().blocks.push(block)
    } else if (mode === 'lead') {
      lead.push(block)
    }
  }

  const makeGlossary = (text: string) => {
    const match = text.match(/^\*\*(.+?)\*\*\s*（(.+?)）\s*[：:]\s*([\s\S]+)$/)
    const term = match ? match[1].trim() : text.split('：')[0].trim()
    const cat = match ? match[2].trim() : '未分类'
    const desc = match ? match[3].trim() : text.replace(/^\*\*.+?\*\*/, '').trim()
    const block = factory(`${term}：${desc}`, 'li',
      `<span class="gloss-term">${escapeHtml(term)}</span>　<span class="gloss-desc">${escapeHtml(desc)}</span>`)
    glossary.push({ cat, term, desc, block })
  }

  for (const token of tokens) {
    const type = token.type as string
    if (type === 'heading') {
      const depth = Number(token.depth ?? 1)
      const text = plain(String(token.text ?? ''))
      if (depth === 1) continue
      if (depth === 2) {
        sawH2 = true
        current = null
        if (text === '速览' || text === '摘要' || text === '核心结论') {
          mode = 'summary'
        } else if (text === '正文' || text === '核心观点与支撑') {
          mode = 'body'
        } else if (text === '速查' || text === '速查表') {
          mode = 'glossary'
        } else {
          mode = 'other'
          current = { heading: text, blocks: [] }
          sections.push(current)
        }
      } else if (depth >= 3) {
        if (mode === 'body' || mode === 'other') {
          current = { heading: text, blocks: [] }
          sections.push(current)
        } else if (text) {
          const block = factory(text, 'raw', `<h3>${escapeHtml(text)}</h3>`)
          push(block)
        }
      }
      continue
    }

    if (type === 'paragraph') {
      const text = String(token.text ?? '')
      if (!text.trim()) continue
      const block = factory(text, 'p', inline(text))
      if (mode === 'glossary') {
        makeGlossary(text)
      } else {
        push(block)
      }
      continue
    }

    if (type === 'blockquote') {
      const text = String(token.text ?? '')
      const block = factory(text, 'quote', sanitize(marked.parse(text) as string))
      push(block)
      continue
    }

    if (type === 'list') {
      const items = (token.items ?? []) as Array<Record<string, unknown>>
      for (const item of items) {
        const text = String(item.text ?? '')
        if (!text.trim()) continue
        if (mode === 'glossary') {
          makeGlossary(text)
        } else if (mode === 'summary') {
          summary.push(factory(text, 'li', inline(text)))
        } else {
          const html = `<span class="li-mark">·</span> ${inline(text)}`
          push(factory(text, 'li', html))
        }
      }
      continue
    }

    if (type === 'table') {
      const raw = String(token.raw ?? '')
      const block = factory(raw, 'raw', sanitize(marked.parse(raw) as string))
      push(block)
      continue
    }

    if (type === 'code') {
      const raw = String(token.raw ?? '')
      const block = factory(raw, 'raw', sanitize(marked.parse(raw) as string))
      push(block)
      continue
    }
  }

  if (!sawH2 && lead.length) {
    sections.unshift({ heading: '', blocks: lead.splice(0, lead.length) })
  }

  const allBlocks: Block[] = [
    ...lead,
    ...summary,
    ...sections.flatMap((section) => section.blocks),
    ...glossary.map((item) => item.block),
  ]

  return {
    lead,
    summary,
    sections,
    glossary,
    allBlocks,
    generic: summary.length === 0 && glossary.length === 0,
  }
}
