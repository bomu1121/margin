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
]

function isHistorical(note) {
  const tags = note.tags || []
  const domain = tags.find((tag) => tag.startsWith('领域/')) || ''
  return domain.includes('历史') || (note.periods || []).length > 0
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

function inferYear(note) {
  // A concrete year in the opening prose is more specific than a dynasty.
  const body = (note.body || '').slice(0, 4000)
  const explicit = body.match(/(?:公元|西元)\s*(\d{1,4})\s*年|前\s*(\d{1,4})\s*年/)
  if (explicit) {
    const value = Number(explicit[1] || explicit[2])
    if (value > 0 && value <= 2026) return explicit[1] ? value : -value
  }
  const text = textOf(note)
  for (const [label, year] of PERIODS) {
    if (text.includes(label)) return year
  }
  return 0
}

function inferPlace(note) {
  // Prefer the title and extracted events over the full body: a long article
  // may mention many places, while the headline usually names the subject.
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
  return {
    panel: {
      id: 'atlas',
      params: {
        year: inferYear(note) || 1100,
        place: inferPlace(note),
      },
    },
  }
}
