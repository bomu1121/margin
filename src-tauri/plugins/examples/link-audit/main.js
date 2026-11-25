// 示例插件：链接清单。只读当前笔记，按域名汇总并找重复。
margin.commands.audit = async (ctx) => {
  const note = await margin.notes.read(ctx.noteId)
  const body = note.body || ''
  const urls = []
  const markdown = /\[([^\]]*)\]\((https?:\/\/[^)\s]+)\)/g
  let match
  while ((match = markdown.exec(body))) {
    urls.push({ label: match[1] || match[2], url: match[2] })
  }
  const bare = body
    .replace(/\[[^\]]*\]\(https?:\/\/[^)\s]+\)/g, ' ')
    .match(/https?:\/\/[^\s<>)"']+/g) || []
  for (const url of bare) urls.push({ label: '', url: url.replace(/[),.;!?，。；、！？）】》]+$/, '') })

  const domainOf = (url) => {
    try {
      return new URL(url).hostname.replace(/^www\./, '')
    } catch {
      return '无法解析'
    }
  }
  const domains = new Map()
  const counts = new Map()
  for (const item of urls) {
    const domain = domainOf(item.url)
    domains.set(domain, (domains.get(domain) || 0) + 1)
    counts.set(item.url, (counts.get(item.url) || 0) + 1)
  }
  const duplicates = [...counts.entries()].filter(([, count]) => count > 1)
  const domainRows = [...domains.entries()].sort((left, right) => right[1] - left[1])

  return {
    markdown: [
      '## 链接清单',
      '',
      `- 链接总数：**${urls.length}**`,
      `- 不同 URL：**${counts.size}**`,
      `- 不同域名：**${domains.size}**`,
      `- 重复 URL：**${duplicates.length}**`,
      '',
      '### 按域名',
      '',
      ...domainRows.map(([domain, count]) => `- ${domain}：${count} 条`),
      ...(duplicates.length
        ? ['', '### 重复 URL', '', ...duplicates.map(([url, count]) => `- ${url}（${count} 次）`)]
        : []),
      '',
      `> 检查对象：${note.title || ctx.noteId}`,
    ].join('\n'),
  }
}
