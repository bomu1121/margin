// Overlay scrollbars: the native bar is hidden on vertical-only scroll areas;
// a slim thumb fades in only when the pointer approaches the right edge.
// This keeps the reading surface quiet without taking scroll control away.

const SCROLL_SELECTOR = [
  '.tree-scroll',
  '.doc-scroll',
  '.rail',
  '.rail-body',
  '.facts-page',
  '.fact-detail-page',
  '.sources-page',
  '.source-detail-page',
  '.source-queue-page',
  '.curation-page',
  '.tag-main',
  '.tag-board',
  '.prod-dialog',
  '.fact-merge-history-list',
  '.source-cite-facts',
  '.search-results',
  '.graph-modal',
].join(',')

function canAttach(element: HTMLElement): boolean {
  if (element.scrollHeight <= element.clientHeight + 2) return false
  if (element.scrollWidth > element.clientWidth + 2) return false
  const style = getComputedStyle(element)
  return style.overflowY === 'auto' || style.overflowY === 'scroll' || style.overflowY === 'overlay'
}

export function initOverlayScrollbars(): () => void {
  if (typeof document === 'undefined') return () => {}
  const root = document.createElement('div')
  root.className = 'yb-scrollbar'
  root.setAttribute('aria-hidden', 'true')
  const thumb = document.createElement('div')
  thumb.className = 'yb-scrollbar-thumb'
  root.appendChild(thumb)
  document.body.appendChild(root)

  let active: HTMLElement | null = null
  let pending: HTMLElement | null = null
  let hoverTimer = 0
  let hideTimer = 0
  let scanTimer = 0
  let frame = 0
  let dragging = false
  let disposed = false
  let dragStartY = 0
  let dragStartTop = 0
  let dragTrackHeight = 0
  let dragThumbHeight = 0

  function update(): void {
    if (!active) return
    const rect = active.getBoundingClientRect()
    const trackTop = rect.top + 6
    const trackHeight = Math.max(0, rect.height - 12)
    const thumbHeight = Math.max(24, Math.min(trackHeight, trackHeight * active.clientHeight / active.scrollHeight))
    const maxScroll = active.scrollHeight - active.clientHeight
    const offset = maxScroll > 0 ? active.scrollTop / maxScroll * (trackHeight - thumbHeight) : 0
    root.style.left = `${Math.round(rect.right - 12)}px`
    root.style.top = `${Math.round(trackTop)}px`
    root.style.height = `${Math.round(trackHeight)}px`
    thumb.style.height = `${Math.round(thumbHeight)}px`
    thumb.style.transform = `translateY(${Math.round(offset)}px)`
  }

  function render(): void {
    if (frame) return
    frame = window.requestAnimationFrame(() => {
      frame = 0
      update()
    })
  }

  function scheduleHide(delay = 900): void {
    window.clearTimeout(hideTimer)
    hideTimer = window.setTimeout(() => {
      if (!dragging) hide()
    }, delay)
  }

  function show(element: HTMLElement): void {
    pending = null
    active = element
    root.classList.add('is-visible')
    render()
    scheduleHide()
  }

  function hide(): void {
    window.clearTimeout(hideTimer)
    root.classList.remove('is-visible')
    active = null
  }

  const resizeObserver = new ResizeObserver(() => {
    if (active) render()
  })

  function scan(): void {
    if (disposed) return
    for (const element of document.querySelectorAll<HTMLElement>(SCROLL_SELECTOR)) {
      const attached = canAttach(element)
      element.classList.toggle('yb-scroll', attached)
      if (attached) resizeObserver.observe(element)
      else resizeObserver.unobserve(element)
    }
    if (active && (!active.isConnected || !canAttach(active))) hide()
  }

  function scheduleScan(): void {
    // Panel drags dispatch this event on every move; reposition the active
    // thumb immediately instead of leaving it at the pre-resize edge.
    if (active) render()
    window.clearTimeout(scanTimer)
    scanTimer = window.setTimeout(scan, 120)
  }

  function nearEdge(x: number, y: number): HTMLElement | null {
    for (const node of document.elementsFromPoint(x, y)) {
      let current = node as HTMLElement | null
      while (current) {
        if (current.classList?.contains('yb-scroll')) {
          const rect = current.getBoundingClientRect()
          if (x >= rect.right - 22 && x <= rect.right + 2 && y >= rect.top && y <= rect.bottom) return current
          break
        }
        current = current.parentElement
      }
    }
    return null
  }

  function onPointerMove(event: PointerEvent): void {
    if (dragging && active) {
      const delta = event.clientY - dragStartY
      const maxScroll = active.scrollHeight - active.clientHeight
      const travel = dragTrackHeight - dragThumbHeight
      if (maxScroll > 0 && travel > 0) active.scrollTop = dragStartTop + delta * (maxScroll / travel)
      render()
      scheduleHide(1200)
      return
    }
    if (root.contains(event.target as Node)) {
      if (active) scheduleHide()
      return
    }
    const target = nearEdge(event.clientX, event.clientY)
    if (target) {
      window.clearTimeout(hoverTimer)
      if (target !== active && target !== pending) {
        pending = target
        hoverTimer = window.setTimeout(() => {
          if (pending) show(pending)
        }, 90)
      }
      scheduleHide()
      return
    }
    pending = null
    window.clearTimeout(hoverTimer)
    scheduleHide(180)
  }

  function onScroll(event: Event): void {
    const target = event.target as HTMLElement | null
    if (!target?.classList?.contains('yb-scroll') || !canAttach(target)) return
    active = target
    root.classList.add('is-visible')
    render()
    scheduleHide(900)
  }

  function onThumbDown(event: PointerEvent): void {
    if (!active) return
    dragging = true
    dragStartY = event.clientY
    dragStartTop = active.scrollTop
    const rect = active.getBoundingClientRect()
    dragTrackHeight = Math.max(0, rect.height - 12)
    dragThumbHeight = thumb.offsetHeight
    root.classList.add('is-dragging')
    thumb.setPointerCapture(event.pointerId)
    event.preventDefault()
  }

  function onThumbUp(): void {
    if (!dragging) return
    dragging = false
    root.classList.remove('is-dragging')
    scheduleHide()
  }

  function onWindowResize(): void {
    render()
    scheduleScan()
  }

  function onDocumentLeave(): void {
    scheduleHide(180)
  }

  const observer = new MutationObserver(scheduleScan)
  observer.observe(document.body, { childList: true, subtree: true })
  window.addEventListener('pointermove', onPointerMove, { passive: true })
  window.addEventListener('scroll', onScroll, true)
  window.addEventListener('resize', onWindowResize)
  window.addEventListener('yb-scroll-rescan', scheduleScan)
  document.documentElement.addEventListener('mouseleave', onDocumentLeave)
  void document.fonts?.ready.then(scan).catch(() => {})
  thumb.addEventListener('pointerdown', onThumbDown)
  thumb.addEventListener('pointerup', onThumbUp)
  thumb.addEventListener('pointercancel', onThumbUp)
  const initialScan = window.setTimeout(scan, 80)

  return () => {
    disposed = true
    window.clearTimeout(initialScan)
    window.clearTimeout(hoverTimer)
    window.clearTimeout(hideTimer)
    window.clearTimeout(scanTimer)
    if (frame) window.cancelAnimationFrame(frame)
    observer.disconnect()
    resizeObserver.disconnect()
    window.removeEventListener('pointermove', onPointerMove)
    window.removeEventListener('scroll', onScroll, true)
    window.removeEventListener('resize', onWindowResize)
    window.removeEventListener('yb-scroll-rescan', scheduleScan)
    document.documentElement.removeEventListener('mouseleave', onDocumentLeave)
    thumb.removeEventListener('pointerdown', onThumbDown)
    thumb.removeEventListener('pointerup', onThumbUp)
    thumb.removeEventListener('pointercancel', onThumbUp)
    for (const element of document.querySelectorAll<HTMLElement>('.yb-scroll')) element.classList.remove('yb-scroll')
    root.remove()
  }
}
