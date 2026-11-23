// Captures the concise-template notes at two widths for layout review.
// usage: node tools/shot-lab-pages.mjs
// The app must be running with CDP on YEBian_LAYOUT_PORT (default 9223).

import fs from 'node:fs'
import path from 'node:path'
import { connectCdp } from './lib/cdp.mjs'

const port = Number(process.env.YEBian_LAYOUT_PORT || 9223)
const keys = ['liubei', 'apec', 'byd']
const cdp = await connectCdp(port)
fs.mkdirSync('shots/lab-style', { recursive: true })

for (const width of [1440, 1024]) {
  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  })
  for (const key of keys) {
    await cdp.evaluate(`location.hash = '#/note/lab-c-${key}'`)
    await new Promise((resolve) => setTimeout(resolve, 1200))
    await cdp.evaluate('document.querySelector(".doc")?.scrollIntoView({block:"start"})')
    await new Promise((resolve) => setTimeout(resolve, 200))
    const shot = await cdp.send('Page.captureScreenshot', { format: 'png' })
    const file = path.resolve(`shots/lab-style/${key}-${width}.png`)
    fs.writeFileSync(file, Buffer.from(shot.data, 'base64'))
    console.log(file)
  }
}
await cdp.send('Emulation.clearDeviceMetricsOverride')
cdp.close()
