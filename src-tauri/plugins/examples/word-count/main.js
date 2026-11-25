// 示例插件：字数统计。只读当前笔记，返回 Markdown。
// Worker 里没有 DOM/Tauri/Node；只能用宿主注入的 margin API。
margin.commands.count = async (ctx) => {
  const note = await margin.notes.read(ctx.noteId)
  let body = note.body || ''
  if (margin.settings && margin.settings.includeCode === false) {
    body = body.replace(/```[\s\S]*?```/g, ' ')
  }
  const cjk = (body.match(/[\u3400-\u4dbf\u4e00-\u9fff]/g) || []).length
  const latinWords = (body.match(/[A-Za-z0-9]+(?:['’-][A-Za-z0-9]+)*/g) || []).length
  const headings = (body.match(/^#{1,6}\s+\S/gm) || []).length
  const h2 = (body.match(/^##\s+\S/gm) || []).length
  const links = (body.match(/\[[^\]]+\]\([^)]+\)/g) || []).length
  const chars = [...body.replace(/\s/g, '')].length
  const minutes = Math.max(1, Math.round(cjk / 400 + latinWords / 220))

  return {
    markdown: [
      '## 字数统计',
      '',
      `- 字符（不含空白）：**${chars.toLocaleString()}**`,
      `- 中文字符：**${cjk.toLocaleString()}**`,
      `- 英文/数字词：**${latinWords.toLocaleString()}**`,
      `- 标题：**${headings}** 个（H2 ${h2} 个）`,
      `- Markdown 链接：**${links}** 个`,
      `- 预计阅读：**${minutes} 分钟**`,
      '',
      `> 统计对象：${note.title || ctx.noteId}`,
    ].join('\n'),
  }
}
