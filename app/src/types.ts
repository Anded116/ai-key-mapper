/** Событие `state` движка, см. «Контракт движок → UI» в docs/architecture.md. */
export interface PadState {
  pad: { model: string; bus: string; battery: number }
  buttons: string[]
  sticks: { left: [number, number]; right: [number, number] }
  triggers: { l2: number; r2: number }
  gyro_dps: [number, number, number]
  touch: { x: number; y: number }[]
  layer: string | null
  outputs: string[]
  profile: string | null
  weapons: { current: number | null; next_double: string | null }
}

/** Строка ленты чата из события `agent`. */
export interface AgentEvent {
  kind: 'text' | 'tool' | 'result' | 'error' | 'done'
  text: string
}
