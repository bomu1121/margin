// Audits rendered text: font size and contrast against the effective
// background, for every visible text node on the current page.
//
// usage: node tools/audit-text.mjs [route]
//   node tools/audit-text.mjs            # audit whatever is on screen
//   node tools/audit-text.mjs graph      # switch to #/graph first
//   node tools/audit-text.mjs note/<id>
//
// Pass the route without a leading "#/" — Git Bash rewrites arguments that
// look like paths, so "#/graph" arrives as "#D:/Develop/Git/graph".
//
// Requires the dev webview to be running with a CDP port:
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npm run tauri dev
//
// Exits non-zero when anything informational is under 12px or under 4.5:1,
// so it can be used as a gate rather than a report you have to read.

const routeArg = process.argv[2] || ''
if (routeArg.includes(':')) {
  console.error(`route argument looks shell-mangled: ${routeArg}`)
  console.error('pass the route without "#/", e.g. "graph" or "note/<id>"')
  process.exit(2)
}
const HASH = routeArg ? `#/${routeArg.replace(/^#?\/?/, '')}` : ''

const list = await (await fetch('http://127.0.0.1:9222/json/list')).json()
const page = list.find((item) => item.type === 'page')
if (!page) throw new Error('no CDP page target')

const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((resolve, reject) => {
  ws.addEventListener('open', resolve, { once: true })
  ws.addEventListener('error', reject, { once: true })
})

let nextId = 0
const pending = new Map()
ws.addEventListener('message', (event) => {
  const message = JSON.parse(String(event.data))
  const entry = pending.get(message.id)
  if (!entry) return
  pending.delete(message.id)
  entry(message)
})

function send(method, params = {}) {
  const callId = ++nextId
  ws.send(JSON.stringify({ id: callId, method, params }))
  return new Promise((resolve, reject) => {
    pending.set(callId, (message) => {
      if (message.error) reject(new Error(JSON.stringify(message.error)))
      else resolve(message.result)
    })
  })
}

async function evaluate(expression) {
  const result = await send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  })
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description ?? 'evaluate failed')
  }
  return result.result.value
}

const AUDIT = String.raw`(() => {
  // Colors can be rgb(), rgba() or color(srgb r g b / a) with 0-1 channels.
  const parse = (value) => {
    if (!value) return null
    const numbers = value.match(/-?[\d.]+/g)
    if (!numbers) return null
    const nums = numbers.map(Number)
    const srgb = value.startsWith('color(')
    const channels = srgb ? nums.slice(0, 3).map((v) => v * 255) : nums.slice(0, 3)
    let alpha = 1
    if (srgb) {
      const slash = value.indexOf('/')
      alpha = slash === -1 ? 1 : Number(value.slice(slash + 1).replace(')', ''))
    } else if (nums.length > 3) {
      alpha = nums[3]
    }
    return { channels, alpha }
  }
  const luminance = (channels) => {
    const linear = channels.map((v) => {
      const c = v / 255
      return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4)
    })
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
  }
  const blend = (top, bottom) => ({
    channels: top.channels.map((v, i) => v * top.alpha + bottom.channels[i] * (1 - top.alpha)),
  })
  const backgroundOf = (el) => {
    let base = parse(getComputedStyle(document.body).backgroundColor)
    if (!base) base = { channels: [255, 255, 255], alpha: 1 }
    const layers = []
    for (let node = el; node && node !== document.documentElement; node = node.parentElement) {
      const layer = parse(getComputedStyle(node).backgroundColor)
      if (layer && layer.alpha > 0) {
        layers.push(layer)
        if (layer.alpha >= 0.99) break
      }
    }
    return layers.reverse().reduce((acc, layer) => blend(layer, acc), base)
  }

  const rows = []
  for (const el of document.querySelectorAll('body *')) {
    const text = Array.from(el.childNodes)
      .filter((node) => node.nodeType === 3)
      .map((node) => node.textContent.trim())
      .join(' ')
      .trim()
    if (!text) continue
    const rect = el.getBoundingClientRect()
    if (rect.width < 1 || rect.height < 1) continue
    const style = getComputedStyle(el)
    if (style.visibility === 'hidden' || style.display === 'none') continue

    const fontSize = parseFloat(style.fontSize)
    const color = parse(style.color)
    if (!color) continue
    const fg = luminance(color.channels)
    const bg = luminance(backgroundOf(el).channels)
    const contrast = (Math.max(fg, bg) + 0.05) / (Math.min(fg, bg) + 0.05)

    rows.push({
      selector: el.className && typeof el.className === 'string'
        ? '.' + el.className.trim().split(/\s+/).join('.')
        : el.tagName.toLowerCase(),
      fontSize: +fontSize.toFixed(1),
      contrast: +contrast.toFixed(2),
      text: text.slice(0, 24).replace(/\s+/g, ' '),
    })
  }
  rows.sort((a, b) => a.fontSize - b.fontSize || a.contrast - b.contrast)
  return {
    route: location.hash || '/',
    dpr: devicePixelRatio,
    viewport: innerWidth + 'x' + innerHeight,
    total: rows.length,
    under10: rows.filter((r) => r.fontSize < 10).length,
    under12: rows.filter((r) => r.fontSize < 12).length,
    lowContrast: rows.filter((r) => r.contrast < 4.5).length,
    rows,
  }
})()`

if (HASH) {
  await evaluate(`new Promise((r) => { location.hash = ${JSON.stringify(HASH)}; setTimeout(r, 1200) })`)
}

const report = await evaluate(AUDIT)
ws.close()

console.log(`route ${report.route}  viewport ${report.viewport} @${report.dpr}x`)
console.log(`text nodes ${report.total}  <10px ${report.under10}  <12px ${report.under12}  <4.5:1 ${report.lowContrast}`)

const offenders = report.rows.filter((row) => row.fontSize < 12 || row.contrast < 4.5)
if (offenders.length) {
  console.log('\nsmallest / lowest contrast:')
  for (const row of offenders.slice(0, 25)) {
    console.log(
      `  ${String(row.fontSize).padStart(5)}px  ${String(row.contrast).padStart(5)}:1  ` +
        `${row.selector.slice(0, 34).padEnd(34)}  ${row.text}`,
    )
  }
  console.log(`\n${offenders.length} of ${report.total} below the floor (12px / 4.5:1)`)
  process.exitCode = 1
} else {
  console.log('\nall text is at or above the floor (12px / 4.5:1)')
}
