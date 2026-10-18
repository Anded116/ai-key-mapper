import type { BindingEntry, Bindings, PadState } from './types'

/**
 * Метки на схеме: какую клавиши игры даёт каждая кнопка. Игра на русской раскладке пишет
 * «нажми Ы» — поэтому буквы крупно по-русски, мелко латиницей. Пока держат кнопку слоя (R2),
 * метки показывают слой.
 */

/** Центры меток в координатах схемы (viewBox 800×520). */
const AT: Record<string, [number, number]> = {
  l2: [118, 37], r2: [682, 37], l1: [116, 75], r1: [684, 75],
  create: [257, 101], options: [543, 101], touchpad: [400, 122], pad_left: [345, 196], pad_right: [455, 196],
  up: [175, 138], right: [252, 205], down: [175, 272], left: [98, 205],
  triangle: [625, 116], circle: [714, 205], cross: [625, 294], square: [528, 205],
  l3: [208, 382], r3: [592, 382], ps: [400, 248], mic: [400, 336],
  mv0: [290, 244], mv1: [228, 305], mv2: [290, 368], mv3: [352, 305],
}

/** ЙЦУКЕН поверх QWERTY: что игра покажет на русской раскладке. */
const RU: Record<string, string> = {
  Q: 'Й', W: 'Ц', E: 'У', R: 'К', T: 'Е', Y: 'Н', U: 'Г', I: 'Ш', O: 'Щ', P: 'З',
  A: 'Ф', S: 'Ы', D: 'В', F: 'А', G: 'П', H: 'Р', J: 'О', K: 'Л', L: 'Д',
  Z: 'Я', X: 'Ч', C: 'С', V: 'М', B: 'И', N: 'Т', M: 'Ь',
  LeftBracket: 'Х', RightBracket: 'Ъ', Semicolon: 'Ж', Quote: 'Э', Comma: 'Б', Period: 'Ю', Backquote: 'Ё',
}

const SYMBOL: Record<string, string> = {
  LeftBracket: '[', RightBracket: ']', Semicolon: ';', Quote: "'", Comma: ',', Period: '.', Backquote: '`',
}

const NAMES: Record<string, string> = {
  Space: 'Пробел', LShift: 'Shift', RShift: 'Shift', LCtrl: 'Ctrl', RCtrl: 'Ctrl', LAlt: 'Alt', RAlt: 'Alt',
  MouseLeft: 'ЛКМ', MouseRight: 'ПКМ', MouseMiddle: 'СКМ', MouseBack: 'Мышь 4', MouseForward: 'Мышь 5',
  WheelUp: 'Колесо ↑', WheelDown: 'Колесо ↓', Backspace: '⌫', Up: '↑', Down: '↓', Left: '←', Right: '→',
  Minus: '-', Equals: '=', Slash: '/', Backslash: '\\', ToggleMovement: 'WASD ↔ стик',
}

const HOW: Record<string, string> = { hold: 'держать ', double: '×2 ', latch: '' }

const SVG = 'http://www.w3.org/2000/svg'

function span(text: string, cls: string): SVGTSpanElement {
  const t = document.createElementNS(SVG, 'tspan')
  t.textContent = text
  t.setAttribute('class', cls)
  return t
}

/** Клавиша: крупно то, что напишет игра, мелко латиница. */
function keySpans(name: string): SVGTSpanElement[] {
  const ru = RU[name]
  if (ru) return [span(ru, 'k'), span(` ${SYMBOL[name] ?? name}`, 'k-sub')]
  const label = NAMES[name] ?? name
  return [span(label, label.length <= 3 ? 'k' : 'k-word')]
}

function entrySpans(e: BindingEntry): SVGTSpanElement[] {
  const out: SVGTSpanElement[] = []
  e.parts.forEach((part, i) => {
    if (i > 0) out.push(span(part.how === 'next' ? ' / ' : ' · ', 'sep'))
    if (HOW[part.how]) out.push(span(HOW[part.how], 'how'))
    part.keys.forEach((k, j) => {
      if (j > 0) out.push(span('+', 'sep'))
      out.push(...keySpans(k))
    })
  })
  return out
}

export function mountLabels(svg: SVGSVGElement) {
  const layer = document.createElementNS(SVG, 'g')
  layer.setAttribute('class', 'labels')
  svg.appendChild(layer)
  let bindings: Bindings | null = null
  let shownLayer: string | null | undefined
  let wasd = false
  let chips: Record<string, SVGGElement> = {}

  const chip = (id: string, e: BindingEntry, inLayer: boolean) => {
    const [x, y] = AT[id]
    const g = document.createElementNS(SVG, 'g')
    g.setAttribute('class', `chip${inLayer ? ' in-layer' : ''}`)
    const rect = document.createElementNS(SVG, 'rect')
    const text = document.createElementNS(SVG, 'text')
    text.setAttribute('x', String(x))
    text.setAttribute('y', String(y + 6))
    text.append(...entrySpans(e))
    g.append(rect, text)
    if (e.note) {
      const title = document.createElementNS(SVG, 'title')
      title.textContent = e.note
      g.appendChild(title)
    }
    layer.appendChild(g)
    const box = text.getBBox()
    const w = Math.max(box.width + 12, 26)
    rect.setAttribute('x', String(x - w / 2))
    rect.setAttribute('y', String(y - 13))
    rect.setAttribute('width', String(w))
    rect.setAttribute('height', '26')
    rect.setAttribute('rx', '7')
    chips[id] = g
  }

  const render = (activeLayer: string | null) => {
    layer.replaceChildren()
    chips = {}
    shownLayer = activeLayer
    if (!bindings) return
    const layerMap = activeLayer ? bindings.layers[activeLayer] ?? {} : {}
    for (const id of Object.keys(AT)) {
      if (id.startsWith('mv')) continue
      const own = layerMap[id]
      const e = own ?? (activeLayer === id ? undefined : bindings.buttons[id])
      if (e) chip(id, e, Boolean(own))
    }
    if (wasd) {
      bindings.move_keys.forEach((k, i) => chip(`mv${i}`, { parts: [{ how: 'press', keys: [k] }], note: null }, false))
    }
  }

  return {
    setBindings(b: Bindings | null) {
      bindings = b
      render(shownLayer ?? null)
    },
    draw(s: PadState) {
      const isWasd = s.engine.movement === 'wasd'
      if (s.layer !== shownLayer || isWasd !== wasd) {
        wasd = isWasd
        render(s.layer)
      }
      const pressed = new Set(s.buttons.map((b) => b.toLowerCase()))
      if (pressed.has('touchpad') && s.touch.length) pressed.add(s.touch[0].x < 0.5 ? 'pad_left' : 'pad_right')
      for (const [id, g] of Object.entries(chips)) g.classList.toggle('on', pressed.has(id))
    },
  }
}
