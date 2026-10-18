/** Событие `state` движка, см. «Контракт движок → UI» в docs/architecture.md. */
export interface PadState {
  pad: { model: string; bus: string; battery: number | null; charging: boolean } | null
  buttons: string[]
  sticks: { left: [number, number]; right: [number, number] }
  triggers: { l2: number; r2: number }
  gyro_dps: [number, number, number]
  touch: { x: number; y: number }[]
  layer: string | null
  outputs: string[]
  profile: string | null
  weapons: { current: number | null; next_double: string | null }
  engine: { focused: boolean; paused: boolean; hidden: boolean; calibrating: boolean; movement: string | null }
}

/** Строка ленты чата из события `agent`. */
export interface AgentEvent {
  kind: 'user' | 'system' | 'text' | 'tool' | 'result' | 'error' | 'done'
  text: string
}

/** Событие `bindings`: что кнопки делают в игре, для меток на схеме. */
export interface Bindings {
  profile: string
  file: string
  prompts: 'ru' | 'en'
  buttons: Record<string, BindingEntry>
  layers: Record<string, Record<string, BindingEntry>>
  move_keys: string[]
}

export interface BindingEntry {
  parts: { how: 'press' | 'tap' | 'hold' | 'double' | 'next' | 'latch'; keys: string[] }[]
  note: string | null
  own: boolean
}
