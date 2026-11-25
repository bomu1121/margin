// 示例插件：标题体检。只读当前笔记，返回结构问题清单。
margin.commands.audit = async (ctx) => {
  const note = await margin.notes.read(ctx.noteId)
  const body = note.body || ''
  const lines = body.split(/\r?\n/)
  const headings = []
  for (let index = 0; index < lines.length; index += 1) {
    const match = /^(#{1,6})\s*(.*)$/.exec(lines[index])
    if (!match) continue
    headings.push({ line: index + 1, level: match[1].length, text: match[2].trim() })
  }

  const empty = headings.filter((item) => !item.text)
  const seen = new Map()
  const duplicates = []
  for (const item of headings) {
    if (!item.text) continue
    const key = item.text.toLowerCase()
    if (seen.has(key)) duplicates.push({ first: seen.get(key), current: item })
    else seen.set(key, item)
  }
  const skips = []
  for (let index = 1; index < headings.length; index += 1) {
    if (headings[index].level > headings[index - 1].level + 1) {
      skips.push({ previous: headings[index - 1], current: headings[index] })
    }
  }
  const h2 = headings.filter((item) => item.level === 2).length
  const issues = empty.length + duplicates.length + skips.length + (h2 > 8 ? 1 : 0)

  const sections = []
  if (empty.length) {
    sections.push(`### 空标题（${empty.length}）`, ...empty.map((item) => `- 第 ${item.line} 行：${'#'.repeat(item.level)}`))
  }
  if (duplicates.length) {
    sections.push(
      `### 重复标题（${duplicates.length}）`,
      ...duplicates.map(
        (item) => `- 「${item.current.text}」：第 ${item.first.line} 行与第 ${item.current.line} 行`,
      ),
    )
  }
  if (skips.length) {
    sections.push(
      `### 跳级（${skips.length}）`,
      ...skips.map(
        (item) =>
          `- 第 ${item.previous.line} 行 H${item.previous.level} → 第 ${item.current.line} 行 H${item.current.level}：${item.current.text || '（空）'}`,
      ),
    )
  }
  if (h2 > 8) sections.push(`### H2 过多（${h2}）`, `- 当前有 ${h2} 个 H2，阅读时可能显得零碎。`)

  return {
    markdown: [
      '## 标题体检',
      '',
      `- 标题总数：**${headings.length}**（H2 ${h2} 个）`,
      `- 发现问题：**${issues}** 个`,
      '',
      issues ? sections.join('\n') : '没有发现空标题、重复标题、跳级或 H2 过多。',
      '',
      `> 检查对象：${note.title || ctx.noteId}`,
    ].join('\n'),
  }
}
