// Small, deterministic text -> map context for whiteboard cards and future
// free-text entries. Keep the event table aligned with the Jiangshan plugin;
// the Worker cannot call this module, so it intentionally mirrors the rules.
export interface MapTextContext {
  year: string
  place: string
  person: string
  event: string
}

export interface MapPoint {
  id: string
  label: string
  place: string
  year: string
  event: string
  detail: string
  approximate: boolean
}

const EVENTS: Array<{ keys: string[]; year: number; place: string; event?: string }> = [
  { keys: ['昆阳之战', '昆阳大捷'], year: 23, place: '昆阳' },
  { keys: ['巨鹿之战'], year: -207, place: '巨鹿' },
  { keys: ['垓下之围', '垓下之战'], year: -202, place: '垓下', event: 'chuhan' },
  { keys: ['官渡之战'], year: 200, place: '官渡' },
  { keys: ['赤壁之战', '赤壁大战'], year: 208, place: '赤壁' },
  { keys: ['夷陵之战', '猇亭之战'], year: 222, place: '夷陵' },
  { keys: ['淝水之战'], year: 383, place: '淝水' },
  { keys: ['玄武门之变'], year: 626, place: '长安' },
  { keys: ['安史之乱'], year: 755, place: '长安', event: 'anshi' },
  { keys: ['陈桥兵变'], year: 960, place: '开封' },
  { keys: ['澶渊之盟'], year: 1005, place: '澶州' },
  { keys: ['王安石变法', '熙宁变法'], year: 1069, place: '开封' },
  { keys: ['青苗法'], year: 1069, place: '开封' },
  { keys: ['靖康之变', '靖康之耻'], year: 1127, place: '开封', event: 'jingkang' },
  { keys: ['靖难之役'], year: 1399, place: '南京' },
  { keys: ['郑和下西洋'], year: 1405, place: '南京' },
  { keys: ['土木堡之变'], year: 1449, place: '土木堡' },
  { keys: ['壬寅宫变'], year: 1542, place: '北京' },
  { keys: ['隗嚣'], year: 32, place: '陇右' },
  { keys: ['蒙古灭金'], year: 1234, place: '蔡州' },
  { keys: ['三藩之乱'], year: 1673, place: '昆明' },
  { keys: ['鸦片战争'], year: 1840, place: '广州' },
  { keys: ['甲午战争', '甲午海战'], year: 1894, place: '威海' },
  { keys: ['辛亥革命', '武昌起义'], year: 1911, place: '武昌' },
  { keys: ['卢沟桥事变', '七七事变'], year: 1937, place: '卢沟桥' },
  { keys: ['南京大屠杀'], year: 1937, place: '南京' },
  { keys: ['长征'], year: 1934, place: '瑞金', event: 'changzheng' },
]

const PERIODS: Array<[string, number]> = [
  ['明嘉靖', 1540], ['明万历', 1600], ['明末', 1630], ['清康熙', 1700],
  ['清乾隆', 1770], ['晚清', 1880], ['东汉初', 30], ['东汉末', 200],
  ['汉末', 200], ['西汉末', -10], ['唐初', 630], ['宋初', 970],
  ['明初', 1370], ['清初', 1650], ['清末', 1900], ['民国初', 1912],
  ['抗战', 1940], ['解放战争', 1948], ['南北朝', 500], ['十六国', 350],
  ['盛唐', 740], ['中唐', 800], ['晚唐', 880], ['北宋', 1100],
  ['南宋', 1200], ['五代', 930], ['春秋', -500], ['战国', -300],
  ['秦朝', -220], ['汉朝', -100], ['隋朝', 600], ['唐朝', 740],
  ['宋朝', 1100], ['元朝', 1300], ['明朝', 1500], ['清朝', 1750],
  ['民国', 1930], ['新中国', 2000], ['现代', 2000],
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
  '陇右', '西城', '蔡州',
]

export function inferMapText(text: string): MapTextContext {
  const body = text || ''
  const explicit = body.match(/(?:公元|西元)\s*(\d{1,4})\s*年|前\s*(\d{1,4})\s*年/)
  if (explicit) {
    const value = Number(explicit[1] || explicit[2])
    if (value > 0 && value <= 2026) {
      return {
        year: String(explicit[1] ? value : -value),
        place: PLACES.find((place) => body.includes(place)) || '',
        person: '',
        event: '',
      }
    }
  }

  const event = EVENTS.find((entry) => entry.keys.some((key) => body.includes(key)))
  if (event) {
    return {
      year: String(event.year),
      place: event.place,
      person: '',
      event: event.event || '',
    }
  }

  // Single-character dynasties are deliberately excluded from free text:
  // “写明” and “美元” must not become 明/元.
  const period = PERIODS.find(([label]) => label.length >= 2 && body.includes(label))
  return {
    year: period ? String(period[1]) : '',
    place: PLACES.find((place) => body.includes(place)) || '',
    person: '',
    event: '',
  }
}

/// Extract all map-worthy places/events for the note list. Saved context is
/// preferred for the initial selection but is not duplicated in the list when
/// an event/place already covers the same target.
export function extractMapPoints(
  text: string,
  saved?: { year?: string; place?: string; event?: string },
): MapPoint[] {
  const body = text || ''
  const base = inferMapText(body)
  const entries: Array<{ index: number; point: MapPoint }> = []
  const seen = new Set<string>()

  const push = (index: number, point: MapPoint) => {
    if (!point.place && !point.event) return
    const key = `${point.place}|${point.year}`
    if (seen.has(key)) return
    seen.add(key)
    entries.push({ index, point })
  }

  for (const event of EVENTS) {
    let hit = ''
    let index = -1
    for (const key of event.keys) {
      const found = body.indexOf(key)
      if (found >= 0 && (index < 0 || found < index)) {
        index = found
        hit = key
      }
    }
    if (index < 0) continue
    push(index, {
      id: `event:${index}:${hit}`,
      label: hit,
      place: event.place,
      year: String(event.year),
      event: event.event || '',
      detail: `${event.year < 0 ? `前${-event.year}` : event.year}年 · ${event.place}`,
      approximate: false,
    })
  }

  for (const place of PLACES) {
    const index = body.indexOf(place)
    if (index < 0) continue
    // An event point already names this place; do not add an approximate
    // second row for the same city with a different year.
    if (entries.some((entry) => entry.point.place === place)) continue
    push(index, {
      id: `place:${index}:${place}`,
      label: place,
      place,
      year: base.year || '',
      event: '',
      detail: base.year ? `约${base.year.replace('-', '前')}年` : '年份未定',
      approximate: Boolean(base.year),
    })
  }

  if (saved?.place) {
    const exists = entries.some(
      (entry) => entry.point.place === saved.place && entry.point.year === String(saved.year || ''),
    )
    if (!exists) {
      entries.unshift({
        index: -1,
        point: {
          id: 'saved',
          label: saved.place,
          place: saved.place,
          year: String(saved.year || ''),
          event: saved.event || '',
          detail: saved.year ? '已保存的地图上下文' : '已保存 · 年份未定',
          approximate: false,
        },
      })
    }
  }

  entries.sort((left, right) => left.index - right.index)
  return entries.slice(0, 12).map((entry) => entry.point)
}
