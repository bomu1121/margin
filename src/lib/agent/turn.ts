import type { Phase } from './types'

// Explicit phase machine: every transition is validated, and the runtime
// records the path. This is the part that keeps a failure from being reported
// as success.
const ALLOWED: Record<Phase, Phase[]> = {
  idle: ['model'],
  model: ['tool', 'done', 'failed', 'model'],
  tool: ['observe'],
  observe: ['model', 'done', 'failed'],
  done: [],
  failed: [],
}

export class TurnMachine {
  phase: Phase = 'idle'
  readonly phases: Phase[] = ['idle']

  to(next: Phase): void {
    if (!ALLOWED[this.phase].includes(next)) {
      throw new Error(`非法阶段跃迁：${this.phase} → ${next}`)
    }
    this.phase = next
    this.phases.push(next)
  }
}

export function canTransition(from: Phase, to: Phase): boolean {
  return ALLOWED[from].includes(to)
}
