import { invoke } from '@tauri-apps/api/core'
import { MOUSE_KEYS, keyFace, keyFromCode } from './keys'
import type { BindingEntry, Bindings } from './types'

/** Правка хоткея одной кнопки: режим, клавиши, описание. Пишет в профиль игры. */

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;")

export const TITLE: Record<string, string> = {
  l1: 'L1', r1: 'R1', l2: 'L2', r2: 'R2', l3: 'L3 — клик левого стика', r3: 'R3 — клик правого стика',
  cross: '✕ Крест', circle: '○ Круг', square: '□ Квадрат', triangle: '△ Треугольник',
  up: 'Крестовина ↑', down: 'Крестовина ↓', left: 'Крестовина ←', right: 'Крестовина →',
  create: 'Create', options: 'Options', ps: 'PS', mic: 'Микрофон',
  pad_left: 'Тачпад — левая половина', pad_right: 'Тачпад — правая половина', touchpad: 'Тачпад',
}

/** Имя кнопки в профиле. */
const PROFILE_NAME: Record<string, string> = { up: 'dpad_up', down: 'dpad_down', left: 'dpad_left', right: 'dpad_right' }

/** Кнопки меню: обычное нажатие длиннее 150 мс, удержание им нужно с запасом. */
const MENU = new Set(['options', 'create', 'touchpad', 'pad_left', 'pad_right', 'mic'])

type Mode = 'press' | 'taphold' | 'double' | 'pair' | 'latch'

const MODES: { mode: Mode; label: string; slots: [string, keyof Edit][] }[] = [
  { mode: 'press', label: 'Нажатие', slots: [['Клавиша', 'press']] },
  { mode: 'taphold', label: 'Тап / удержание', slots: [['Тап', 'tap'], ['Удержание', 'hold']] },
  { mode: 'double', label: 'Двойное', slots: [['Одно нажатие', 'press'], ['Двойное', 'double']] },
  { mode: 'pair', label: 'Пара', slots: [['Первое нажатие', 'press'], ['Повторное', 'next']] },
  { mode: 'latch', label: 'Клик до остановки', slots: [['Клавиша', 'latch']] },
]

interface Edit {
  press: string[]
  tap: string[]
  hold: string[]
  double: string[]
  next: string[]
  latch: string[]
  hold_ms?: number
}

function fromEntry(e: BindingEntry | undefined): { mode: Mode; edit: Edit } {
  const edit: Edit = { press: [], tap: [], hold: [], double: [], next: [], latch: [] }
  for (const p of e?.parts ?? []) edit[p.how] = [...p.keys]
  const has = (k: keyof Edit) => (edit[k] as string[]).length > 0
  const mode: Mode = has('latch') ? 'latch' : has('next') ? 'pair' : has('tap') || has('hold') ? 'taphold' : has('double') ? 'double' : 'press'
  return { mode, edit }
}

export function mountEditor(root: HTMLElement, getBindings: () => Bindings | null, onSaved: () => void) {
  let id = ''
  let layer: string | null = null
  let mode: Mode = 'press'
  let edit: Edit = fromEntry(undefined).edit
  let entry: BindingEntry | undefined
  let listening: keyof Edit | null = null

  const close = () => {
    root.hidden = true
    listening = null
    window.removeEventListener('keydown', onKey, true)
  }

  const onKey = (ev: KeyboardEvent) => {
    if (!listening) {
      if (ev.key === 'Escape') close()
      return
    }
    ev.preventDefault()
    ev.stopPropagation()
    const key = keyFromCode(ev.code)
    if (!key) return
    ;(edit[listening] as string[]) = [key]
    listening = null
    render()
  }

  const slot = (label: string, field: keyof Edit) => {
    const keys = edit[field] as string[]
    const active = listening === field
    const face = keys.length
      ? keys.map((k) => { const f = keyFace(k); return `<span class="cap${f.wide ? ' wide' : ''}"><b>${f.main}</b>${f.sub ? `<i>${f.sub}</i>` : ''}</span>` }).join('<span class="plus">+</span>')
      : '<span class="slot-empty">не задано</span>'
    return `<div class="slot${active ? ' listening' : ''}" data-field="${field}">
      <span class="slot-label">${label}</span>
      <button class="slot-key" data-act="listen">${active ? '<span class="slot-wait">нажми клавишу…</span>' : face}</button>
      ${keys.length ? '<button class="icon-btn" data-act="clear" title="Очистить">×</button>' : ''}
    </div>`
  }

  const render = () => {
    const b = getBindings()
    const name = PROFILE_NAME[id] ?? id
    const spec = MODES.find((m) => m.mode === mode)!
    const modes = MODES.filter((m) => m.mode !== 'pair' || mode === 'pair' || layer).filter((m) => m.mode !== 'latch' || mode === 'latch' || id === 'l3')
    root.innerHTML = `<div class="dialog" role="dialog" aria-label="Хоткей">
      <header>
        <div><div class="dialog-title">${layer ? 'R2 + ' : ''}${TITLE[id] ?? id}</div>
        <div class="dialog-sub">${b?.file.startsWith('_') ? 'Стандарт — для всех игр' : b?.profile ?? ''} · ${b?.file ?? ''}</div></div>
        <button class="icon-btn" data-act="close" title="Закрыть">×</button>
      </header>
      <label class="field"><span>Что делает в игре</span>
        <input id="ed-note" type="text" value="${esc((document.getElementById('ed-note') as HTMLInputElement | null)?.value ?? entry?.note ?? '')}" placeholder="например: сон" /></label>
      <div class="segmented">${modes.map((m) => `<button data-mode="${m.mode}" class="${m.mode === mode ? 'active' : ''}">${m.label}</button>`).join('')}</div>
      <div class="slots">${spec.slots.map(([label, field]) => slot(label, field)).join('')}</div>
      ${listening ? `<div class="mouse-keys"><span>или мышь:</span>${MOUSE_KEYS.map((k) => `<button data-mouse="${k}" title="${keyFace(k).title}">${keyFace(k).main}</button>`).join('')}</div>` : ''}
      ${mode === 'taphold' ? `<div class="hint">Удержание срабатывает через ${MENU.has(id) ? 500 : 150} мс.</div>` : ''}
      <footer>
        <div class="left">
          ${entry?.own && !b?.file.startsWith('_') ? '<button class="ghost" data-act="reset" title="Убрать правку игры, вернуть привязку из стандарта">Как в стандарте</button>' : ''}
          <button class="ghost" data-act="unbind">Ничего не делать</button>
        </div>
        <div class="right"><button class="ghost" data-act="close">Отмена</button><button class="primary" data-act="save">Сохранить</button></div>
      </footer>
      <div class="dialog-error" id="ed-error"></div>
    </div>`
    root.dataset.button = name
  }

  const save = async (e: Partial<Edit>) => {
    const note = (document.getElementById('ed-note') as HTMLInputElement).value
    const spec = MODES.find((m) => m.mode === mode)!
    const out: Partial<Edit> = {}
    for (const [, field] of spec.slots) out[field] = e[field] as never
    if (mode === 'taphold' && MENU.has(id)) out.hold_ms = 500
    try {
      await invoke('set_binding', { file: getBindings()!.file, button: PROFILE_NAME[id] ?? id, layer, edit: out, note })
      close()
      onSaved()
    } catch (err) {
      document.getElementById('ed-error')!.textContent = String(err)
    }
  }

  root.addEventListener('click', async (ev) => {
    if (ev.target === root) return close()
    const t = (ev.target as HTMLElement).closest<HTMLElement>('button')
    if (!t) return
    const field = t.closest<HTMLElement>('.slot')?.dataset.field as keyof Edit | undefined
    if (t.dataset.mode) {
      const keys = edit[MODES.find((m) => m.mode === mode)!.slots[0][1]] as string[]
      mode = t.dataset.mode as Mode
      const first = MODES.find((m) => m.mode === mode)!.slots[0][1]
      if (!(edit[first] as string[]).length) (edit[first] as string[]) = [...keys]
      listening = null
      render()
    } else if (t.dataset.mouse && listening) {
      ;(edit[listening] as string[]) = [t.dataset.mouse]
      listening = null
      render()
    } else if (t.dataset.act === 'listen' && field) {
      listening = listening === field ? null : field
      render()
    } else if (t.dataset.act === 'clear' && field) {
      ;(edit[field] as string[]) = []
      render()
    } else if (t.dataset.act === 'close') {
      close()
    } else if (t.dataset.act === 'save') {
      await save(edit)
    } else if (t.dataset.act === 'unbind') {
      mode = 'press'
      await save({ press: [] })
    } else if (t.dataset.act === 'reset') {
      try {
        await invoke('reset_binding', { file: getBindings()!.file, button: PROFILE_NAME[id] ?? id, layer })
        close()
        onSaved()
      } catch (err) {
        document.getElementById('ed-error')!.textContent = String(err)
      }
    }
  })

  return {
    open(buttonId: string, layerName: string | null) {
      const b = getBindings()
      if (!b) return
      id = buttonId
      layer = layerName
      entry = layer ? b.layers[layer]?.[id] : b.buttons[id]
      ;({ mode, edit } = fromEntry(entry))
      const note = document.getElementById('ed-note') as HTMLInputElement | null
      if (note) note.value = entry?.note ?? ''
      root.innerHTML = ''
      listening = (edit[MODES.find((m) => m.mode === mode)!.slots[0][1]] as string[]).length ? null : MODES.find((m) => m.mode === mode)!.slots[0][1]
      render()
      root.hidden = false
      window.addEventListener('keydown', onKey, true)
    },
  }
}
