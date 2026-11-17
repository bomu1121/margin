export type FigureKind = 'flow' | 'layers' | 'bar' | 'quote'

export interface FigureSpec {
  kind: FigureKind
  title: string
  note: string
  lines: string[]
}

export function parseFigure(raw: string): FigureSpec
export function renderFigure(spec: FigureSpec, width?: number): { html: string; caption: string }
