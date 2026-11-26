// 江山时序面板插件：只做一件事——把当前笔记压成 (year, place) 两个语义参数。
// 地图本身在跨域 sandbox iframe 里加载；这里不接触网络、DOM 或宿主界面。

const PERIODS = [
  ['明嘉靖', 1540],
  ['明万历', 1600],
  ['明末', 1630],
  ['清康熙', 1700],
  ['清乾隆', 1770],
  ['晚清', 1880],
  ['东汉初', 30],
  ['东汉末', 200],
  ['汉末', 200],
  ['西汉末', -10],
  ['唐初', 630],
  ['宋初', 970],
  ['明初', 1370],
  ['明末', 1630],
  ['清初', 1650],
  ['清末', 1900],
  ['民国初', 1912],
  ['抗战', 1940],
  ['解放战争', 1948],
  ['三国', 250],
  ['南北朝', 500],
  ['十六国', 350],
  ['盛唐', 740],
  ['中唐', 800],
  ['晚唐', 880],
  ['北宋', 1100],
  ['南宋', 1200],
  ['五代', 930],
  ['春秋', -500],
  ['战国', -300],
  ['秦', -220],
  ['西汉', -100],
  ['东汉', 100],
  ['西晋', 290],
  ['东晋', 380],
  ['隋', 600],
  ['唐', 740],
  ['元', 1300],
  ['明', 1500],
  ['清', 1750],
  ['秦朝', -220],
  ['汉朝', -100],
  ['隋朝', 600],
  ['唐朝', 740],
  ['宋朝', 1100],
  ['元朝', 1300],
  ['明朝', 1500],
  ['清朝', 1750],
  ['民国', 1930],
  ['新中国', 2000],
  ['现代', 2000],
]

const PLACES = [
  '昆阳', '长安', '洛阳', '建康', '江陵', '赤壁', '官渡', '巨鹿', '垓下', '五丈原',
  '祁山', '襄阳', '樊城', '许昌', '邺城', '成都', '夷陵', '合肥', '寿春',
  '宛城', '下邳', '彭城', '广固', '开封', '临安', '北京', '南京', '杭州',
  '山海关', '潼关', '函谷关', '剑门关', '雁门关', '玉门关', '阳关', '蓝关',
  '虎牢关', '牧野', '鸣条', '长平', '淝水', '黄州', '惠州', '儋州', '湖州',
  '潮州', '秦州', '凉州', '敦煌', '河西', '辽东', '淮南', '淮北', '荆州',
  '扬州', '徐州', '豫州', '冀州', '青州', '兖州', '并州', '幽州', '益州',
  '交州', '江州', '浔阳', '白帝城', '马嵬坡', '兰亭', '岳阳楼', '滕王阁',
  '五台山', '泰山', '华山', '庐山', '洞庭湖', '鄱阳湖', '太湖', '黄河',
  '大运河', '京口', '瓜洲', '采石', '钟离', '寿阳', '江夏', '夏口',
  '柴桑', '樊口', '猇亭', '街亭', '陈仓', '斜谷', '子午谷', '阳平关',
  '汉中', '武都', '阴平', '剑阁', '绵竹', '涪城', '永安', '白帝',
  '秭归', '竟陵', '武昌', '建业', '广陵', '小沛', '谯郡', '邺',
  '中山', '常山', '邯郸', '蓟城', '襄平', '晋阳', '平城', '统万城',
  '姑臧', '酒泉', '张掖', '武威', '金城', '西平', '鄯善', '龟兹',
  '疏勒', '于阗', '碎叶', '怛罗斯', '安西', '北庭', '轮台', '高昌',
  '交河', '伊犁', '雅克萨', '尼布楚', '台湾', '澎湖', '琉球',
  '澶州', '土木堡', '威海', '卢沟桥', '瑞金', '昆明',
]

// A small, auditable event gazetteer. The model was too conservative to infer
// 23 AD for 昆阳之战 from the note alone; these common events give the rule
// layer a second anchor without allowing free-form external guesses.
const EVENTS = [
  { keys: ['昆阳之战', '昆阳大捷'], year: 23, place: '昆阳' },
  { keys: ['巨鹿之战'], year: -207, place: '巨鹿' },
  { keys: ['垓下之围', '垓下之战'], year: -202, place: '垓下', eventKey: 'chuhan' },
  { keys: ['官渡之战'], year: 200, place: '官渡' },
  { keys: ['赤壁之战', '赤壁大战'], year: 208, place: '赤壁' },
  { keys: ['夷陵之战', '猇亭之战'], year: 222, place: '夷陵' },
  { keys: ['淝水之战'], year: 383, place: '淝水' },
  { keys: ['玄武门之变'], year: 626, place: '长安' },
  { keys: ['安史之乱'], year: 755, place: '长安', eventKey: 'anshi' },
  { keys: ['陈桥兵变'], year: 960, place: '开封' },
  { keys: ['澶渊之盟'], year: 1005, place: '澶州' },
  { keys: ['王安石变法', '熙宁变法'], year: 1069, place: '开封' },
  { keys: ['青苗法'], year: 1069, place: '开封' },
  { keys: ['靖康之变', '靖康之耻'], year: 1127, place: '开封', eventKey: 'jingkang' },
  { keys: ['靖难之役'], year: 1399, place: '南京' },
  { keys: ['郑和下西洋'], year: 1405, place: '南京' },
  { keys: ['土木堡之变'], year: 1449, place: '土木堡' },
  { keys: ['壬寅宫变'], year: 1542, place: '北京' },
  { keys: ['三藩之乱'], year: 1673, place: '昆明' },
  { keys: ['鸦片战争'], year: 1840, place: '广州' },
  { keys: ['甲午战争', '甲午海战'], year: 1894, place: '威海' },
  { keys: ['辛亥革命', '武昌起义'], year: 1911, place: '武昌' },
  { keys: ['卢沟桥事变', '七七事变'], year: 1937, place: '卢沟桥' },
  { keys: ['南京大屠杀'], year: 1937, place: '南京' },
  { keys: ['长征'], year: 1934, place: '瑞金', eventKey: 'changzheng' },
]

function isHistorical(note) {
  const tags = note.tags || []
  const domain = tags.find((tag) => tag.startsWith('领域/')) || ''
  // Some generated notes use periods for publication dates (2025-08). Those
  // are not historical periods and must not get a map button.
  const periods = (note.periods || []).filter(
    (period) => !/^\d{4}(?:-\d{2}){0,2}$/.test(String(period).trim()),
  )
  return domain.includes('历史') || periods.length > 0
}

function textOf(note) {
  // Deliberately excludes the body: modern prose contains "元""明" etc. as
  // ordinary characters (e.g. 美元), which would otherwise fake a dynasty.
  return [
    note.title || '',
    ...(note.periods || []),
    ...(note.people || []),
    ...(note.events || []),
    ...(note.tags || []),
  ].join('\n')
}

function eventHit(note) {
  const primary = [note.title || '', ...(note.events || [])].join('\n')
  for (const event of EVENTS) {
    if (event.keys.some((key) => primary.includes(key))) return event
  }
  const body = (note.body || '').split('## 相关笔记')[0].slice(0, 4000)
  for (const event of EVENTS) {
    if (event.keys.some((key) => body.includes(key))) return event
  }
  return null
}

function inferYear(note) {
  // A concrete year in the opening prose is more specific than a dynasty.
  const body = (note.body || '').slice(0, 4000)
  const explicit = body.match(/(?:公元|西元)\s*(\d{1,4})\s*年|前\s*(\d{1,4})\s*年/)
  if (explicit) {
    const value = Number(explicit[1] || explicit[2])
    if (value > 0 && value <= 2026) {
      return { year: explicit[1] ? value : -value, source: 'explicit' }
    }
  }
  const event = eventHit(note)
  if (event) return { year: event.year, source: 'event' }
  // Period metadata/tags are curated, so single-character dynasties are safe
  // there. In free text, "写明" / "美元" would otherwise match 明/元 and
  // invent a dynasty; only multi-character labels are allowed from the title.
  const metaText = [...(note.periods || []), ...(note.tags || [])].join('\n')
  for (const [label, year] of PERIODS) {
    if (metaText.includes(label)) return { year, source: 'period' }
  }
  const titleText = [note.title || '', ...(note.people || []), ...(note.events || [])].join('\n')
  for (const [label, year] of PERIODS) {
    if (label.length >= 2 && titleText.includes(label)) return { year, source: 'period' }
  }
  return { year: 0, source: '' }
}

function inferPlace(note) {
  // Prefer a named event, then the title/events, over the full body: a long
  // article may mention many places, while the headline usually names the subject.
  const event = eventHit(note)
  if (event) return event.place
  const primary = [note.title || '', ...(note.events || [])].join('\n')
  for (const place of PLACES) {
    if (primary.includes(place)) return place
  }
  const body = (note.body || '')
    .split('## 相关笔记')[0]
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .slice(0, 3000)
  for (const place of PLACES) {
    if (body.includes(place)) return place
  }
  return ''
}

margin.commands.open = async (ctx) => {
  const note = await margin.notes.read(ctx.noteId)
  if (!isHistorical(note)) {
    return { message: '这篇笔记不是历史内容，江山时序没有可定位的年代。' }
  }
  const { year, source: yearSource } = inferYear(note)
  const place = inferPlace(note)
  const event = eventHit(note)
  return {
    panel: {
      id: 'atlas',
      params: {
        // No 1100 preview: an empty field plus `fallback` lets the host say
        // "没有定位到" instead of showing a plausible but wrong year.
        year: year || '',
        yearSource,
        place,
        person: '',
        event: event?.eventKey || '',
        source: 'rule',
        fallback: !year ? 'year' : !place ? 'place' : '',
      },
    },
  }
}
