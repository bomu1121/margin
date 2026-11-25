// Image cards paint the bitmap in a frame that excludes the 1px card border and the 23px grip.
// Resizing keeps that frame at the source aspect so `object-fit: contain` never letterboxes.
export const IMAGE_CARD_BORDER = 2
export const IMAGE_CARD_HEADER = 23
export const IMAGE_FRAME_MIN_W = 160
export const IMAGE_FRAME_MIN_H = 40

export function imageSourceAspect(dataJson: string | null | undefined): number | null {
  try {
    const data = JSON.parse(dataJson || '{}') as { width?: number; height?: number }
    if (data.width && data.height && data.width > 0 && data.height > 0) {
      return data.width / data.height
    }
  } catch {
    // Old or malformed data has no usable source size.
  }
  return null
}

export function imageAspectOf(dataJson: string | null | undefined): number {
  return imageSourceAspect(dataJson) ?? 1
}

export function imageFrameSize(cardW: number, cardH: number): { width: number; height: number } {
  return {
    width: Math.max(1, cardW - IMAGE_CARD_BORDER),
    height: Math.max(1, cardH - IMAGE_CARD_HEADER - IMAGE_CARD_BORDER),
  }
}

/** Shrink a frame to the source ratio, the way contain does, then apply the minimums. */
export function fitImageFrame(frameW: number, frameH: number, aspect: number): { width: number; height: number } {
  return clampImageFrame(Math.min(frameW, frameH * aspect), aspect)
}

export function clampImageFrame(width: number, aspect: number): { width: number; height: number } {
  let nextW = Math.max(IMAGE_FRAME_MIN_W, width)
  let nextH = nextW / aspect
  if (nextH < IMAGE_FRAME_MIN_H) {
    nextH = IMAGE_FRAME_MIN_H
    nextW = nextH * aspect
  }
  return { width: nextW, height: nextH }
}

export function imageCardSize(frameW: number, frameH: number): { w: number; h: number } {
  return {
    w: frameW + IMAGE_CARD_BORDER,
    h: frameH + IMAGE_CARD_HEADER + IMAGE_CARD_BORDER,
  }
}
