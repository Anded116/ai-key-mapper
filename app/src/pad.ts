import { keyFace } from './keys'
import type { BindingEntry, Bindings, PadState } from './types'

/**
 * Геймпад с раскладкой прямо на кнопках: на каждой — клавиша, которую она шлёт в игру (буквы
 * по-русски, как их пишет игра). Наведение и нажатие выбирают кнопку, клик — правка.
 */

const SVG = 'http://www.w3.org/2000/svg'
const DPAD = { x: 219, y: 262 }
const FACE = { x: 781, y: 262 }
const STICK_L = { x: 362, y: 392 }
const STICK_R = { x: 638, y: 392 }
const STICK_TRAVEL = 20
const TOUCH = { x: 355, y: 122, w: 290, h: 163 }
const TRIGGER_W = 210

const BODY = `M 287 125 C 412 110, 587 110, 712 125 C 812 135, 875 162, 900 237 C 931 337, 962 475, 950 550
  C 940 612, 862 625, 825 587 C 787 550, 750 475, 700 440 C 650 415, 350 415, 300 440
  C 250 475, 212 550, 175 587 C 137 625, 60 612, 50 550 C 37 475, 69 337, 100 237 C 125 162, 187 135, 287 125 Z`

/** Где и как рисовать подпись каждой кнопки. `cap` — плашка рядом с мелкой кнопкой. */
const LABELS: Record<string, { x: number; y: number; w: number; cap?: boolean }> = {
  l2: { x: 280, y: 44, w: 190 }, r2: { x: 720, y: 44, w: 190 },
  l1: { x: 280, y: 91, w: 190 }, r1: { x: 720, y: 91, w: 190 },
  create: { x: 319, y: 153, w: 40 }, options: { x: 681, y: 153, w: 40 },
  pad_left: { x: 427, y: 205, w: 130 }, pad_right: { x: 573, y: 205, w: 130 },
  up: { x: DPAD.x, y: DPAD.y - 46, w: 46 }, down: { x: DPAD.x, y: DPAD.y + 46, w: 46 },
  left: { x: DPAD.x - 46, y: DPAD.y, w: 46 }, right: { x: DPAD.x + 46, y: DPAD.y, w: 46 },
  triangle: { x: FACE.x, y: FACE.y - 64, w: 58 }, circle: { x: FACE.x + 64, y: FACE.y, w: 58 },
  cross: { x: FACE.x, y: FACE.y + 64, w: 58 }, square: { x: FACE.x - 64, y: FACE.y, w: 58 },
  l3: { x: STICK_L.x, y: STICK_L.y, w: 70 }, r3: { x: STICK_R.x, y: STICK_R.y, w: 70 },
  ps: { x: 500, y: 342, w: 36 }, mic: { x: 500, y: 386, w: 64 },
  mv0: { x: STICK_L.x, y: STICK_L.y - 84, w: 40, cap: true }, mv1: { x: STICK_L.x - 86, y: STICK_L.y, w: 40, cap: true },
  mv2: { x: STICK_L.x, y: STICK_L.y + 84, w: 40, cap: true }, mv3: { x: STICK_L.x + 86, y: STICK_L.y, w: 40, cap: true },
}

function dpad(): string {
  const arm = '-25,-76 25,-76 25,-30 0,-12 -25,-30'
  return (['up', 'right', 'down', 'left'] as const)
    .map((b, i) => `<polygon class="btn" data-btn="${b}" data-id="${b}" points="${arm}"
      transform="translate(${DPAD.x} ${DPAD.y}) rotate(${i * 90})" />`)
    .join('')
}

function face(): string {
  const at = { triangle: [0, -64], circle: [64, 0], cross: [0, 64], square: [-64, 0] } as const
  const glyph = {
    triangle: '<polygon points="0,-5 4.5,3.5 -4.5,3.5" />',
    circle: '<circle r="4.2" />',
    cross: '<path d="M -3.7 -3.7 L 3.7 3.7 M 3.7 -3.7 L -3.7 3.7" />',
    square: '<rect x="-3.7" y="-3.7" width="7.4" height="7.4" />',
  }
  return (Object.keys(at) as (keyof typeof at)[])
    .map((b) => `<g transform="translate(${FACE.x + at[b][0]} ${FACE.y + at[b][1]})">
      <circle class="btn" data-btn="${b}" data-id="${b}" r="33" />
      <g class="glyph glyph-${b}" transform="translate(0 -21)">${glyph[b]}</g></g>`)
    .join('')
}

function stick(id: 'l' | 'r', c: { x: number; y: number }): string {
  return `<circle class="stick-base" cx="${c.x}" cy="${c.y}" r="62" />
    <g id="stick-${id}"><circle class="btn stick-cap" data-btn="${id}3" data-id="${id}3" cx="${c.x}" cy="${c.y}" r="40" /></g>`
}

/** Текст подписи: крупно клавиша, мелко латиница или второе действие. */
function label(e: BindingEntry, maxW: number, cap: boolean): SVGGElement {
  const g = document.createElementNS(SVG, 'g')
  const first = e.parts[0]
  const second = e.parts[1]
  const text = document.createElementNS(SVG, 'text')
  text.setAttribute('class', 'k')
  // латиница — мелким верхним индексом; следующий кусок возвращается на базовую линию
  let raised = false
  const put = (s: string, cls: string, sup = false) => {
    const t = tspan(s, cls)
    if (sup) {
      t.setAttribute('dy', '-8')
      t.setAttribute('dx', '2')
    } else if (raised) {
      t.setAttribute('dy', '8')
    }
    raised = sup
    text.appendChild(t)
  }
  const face = (k: string) => {
    const f = keyFace(k)
    put(f.main, f.wide ? 'k-word' : 'k-main')
    if (f.sub) put(f.sub, 'k-sub', true)
  }
  first.keys.forEach((k, i) => {
    if (i > 0) put(' + ', 'k-sep')
    face(k)
  })
  if (second && second.how === 'next') {
    put(' / ', 'k-sep')
    second.keys.forEach(face)
  }
  g.appendChild(text)
  const extra = second && second.how !== 'next' ? second : first.how === 'latch' ? first : null
  if (extra) {
    const t = document.createElementNS(SVG, 'text')
    t.setAttribute('class', 'k-extra')
    t.setAttribute('y', '15')
    t.textContent = extra.how === 'latch' ? 'клик' : `${extra.how === 'hold' ? 'держать' : '×2'} ${extra.keys.map((k) => keyFace(k).main).join('+')}`
    g.appendChild(t)
    text.setAttribute('y', '-3')
  }
  // подгоняем под кнопку: ужимаем, если не влезает
  requestAnimationFrame(() => {
    const box = g.getBBox()
    if (box.width > maxW) g.setAttribute('transform', `${g.getAttribute('transform') ?? ''} scale(${(maxW / box.width).toFixed(3)})`)
    if (cap) {
      const r = document.createElementNS(SVG, 'rect')
      const w = Math.min(box.width, maxW) + 14
      r.setAttribute('class', 'k-cap')
      r.setAttribute('x', String(-w / 2))
      r.setAttribute('y', String(-15))
      r.setAttribute('width', String(w))
      r.setAttribute('height', extra ? '36' : '28')
      r.setAttribute('rx', '7')
      g.parentElement?.insertBefore(r, g)
    }
  })
  return g
}

function tspan(text: string, cls: string): SVGTSpanElement {
  const t = document.createElementNS(SVG, 'tspan')
  t.textContent = text
  t.setAttribute('class', cls)
  return t
}

export function mountPad(root: HTMLElement, onHover: (id: string | null) => void, onPick: (id: string) => void) {
  root.innerHTML = `<svg viewBox="30 8 940 630" class="pad" xmlns="http://www.w3.org/2000/svg">
    <rect class="btn trigger" data-btn="l2" data-id="l2" x="175" y="20" width="${TRIGGER_W}" height="48" rx="16" />
    <rect class="trigger-fill" id="fill-l2" x="175" y="20" width="0" height="48" rx="16" />
    <rect class="btn trigger" data-btn="r2" data-id="r2" x="615" y="20" width="${TRIGGER_W}" height="48" rx="16" />
    <rect class="trigger-fill" id="fill-r2" x="615" y="20" width="0" height="48" rx="16" />
    <rect class="btn" data-btn="l1" data-id="l1" x="175" y="75" width="${TRIGGER_W}" height="32" rx="16" />
    <rect class="btn" data-btn="r1" data-id="r1" x="615" y="75" width="${TRIGGER_W}" height="32" rx="16" />
    <path class="body" d="${BODY}" />
    <rect class="btn touchpad" data-btn="touchpad" x="${TOUCH.x}" y="${TOUCH.y}" width="${TOUCH.w}" height="${TOUCH.h}" rx="22" />
    <line class="touch-split" x1="500" y1="${TOUCH.y + 22}" x2="500" y2="${TOUCH.y + TOUCH.h - 22}" />
    <rect class="hit" data-id="pad_left" x="${TOUCH.x}" y="${TOUCH.y}" width="${TOUCH.w / 2}" height="${TOUCH.h}" />
    <rect class="hit" data-id="pad_right" x="500" y="${TOUCH.y}" width="${TOUCH.w / 2}" height="${TOUCH.h}" />
    <g id="touches"></g>
    <rect class="btn" data-btn="create" data-id="create" x="294" y="126" width="50" height="54" rx="13" />
    <rect class="btn" data-btn="options" data-id="options" x="656" y="126" width="50" height="54" rx="13" />
    <circle class="btn" data-btn="ps" data-id="ps" cx="500" cy="342" r="22" />
    <rect class="btn" data-btn="mic" data-id="mic" x="462" y="372" width="76" height="28" rx="14" />
    ${dpad()}${face()}${stick('l', STICK_L)}${stick('r', STICK_R)}
    <g id="labels"></g>
  </svg>`

  const svg = root.querySelector('svg')!
  const shapes = [...svg.querySelectorAll<SVGElement>('[data-btn]')]
  const hits = [...svg.querySelectorAll<SVGElement>('[data-id]')]
  const layer = svg.querySelector<SVGGElement>('#labels')!
  const touches = svg.querySelector<SVGGElement>('#touches')!
  const caps = { l: svg.querySelector('#stick-l')!, r: svg.querySelector('#stick-r')! }
  const fills = { l2: svg.querySelector('#fill-l2')!, r2: svg.querySelector('#fill-r2')! }
  let labels: Record<string, SVGGElement> = {}

  for (const el of hits) {
    el.addEventListener('mouseenter', () => onHover(el.dataset.id!))
    el.addEventListener('mouseleave', () => onHover(null))
    el.addEventListener('click', () => onPick(el.dataset.id!))
  }

  /** Подписи для профиля и показанного слоя: кнопки слоя поверх, остальные приглушены. */
  const setLabels = (b: Bindings | null, view: string | null, wasd: boolean) => {
    layer.replaceChildren()
    labels = {}
    if (!b) return
    const layerMap = view ? b.layers[view] ?? {} : {}
    const add = (id: string, e: BindingEntry | undefined, cls: string) => {
      const at = LABELS[id]
      if (!at || !e || !e.parts.length || !e.parts[0].keys.length) return
      const g = document.createElementNS(SVG, 'g')
      g.setAttribute('class', `label ${cls}`)
      g.setAttribute('transform', `translate(${at.x} ${at.y})`)
      g.dataset.id = id
      g.appendChild(label(e, at.w, Boolean(at.cap)))
      layer.appendChild(g)
      labels[id] = g
    }
    for (const id of Object.keys(LABELS)) {
      if (id.startsWith('mv')) continue
      if (b.layers[id] && !b.buttons[id]) {
        add(id, { parts: [{ how: 'press', keys: ['слой'] }], note: null, own: false }, view === id ? 'layer-key' : 'layer-key idle')
        continue
      }
      const own = layerMap[id]
      add(id, own ?? b.buttons[id], own ? 'in-layer' : view ? 'dim' : '')
    }
    if (wasd) b.move_keys.forEach((k, i) => add(`mv${i}`, { parts: [{ how: 'press', keys: [k] }], note: null, own: false }, view ? 'dim' : 'move'))
  }

  const select = (id: string | null) => {
    for (const el of hits) el.classList.toggle('selected', el.dataset.id === id)
  }

  const draw = (s: PadState) => {
    const pressed = new Set(s.buttons.map((b) => b.toLowerCase()))
    if (pressed.has('touchpad') && s.touch.length) pressed.add(s.touch[0].x < 0.5 ? 'pad_left' : 'pad_right')
    for (const el of shapes) el.classList.toggle('on', pressed.has(el.dataset.btn!))
    for (const [id, g] of Object.entries(labels)) g.classList.toggle('on', pressed.has(id))
    caps.l.setAttribute('transform', `translate(${(s.sticks.left[0] * STICK_TRAVEL).toFixed(1)} ${(-s.sticks.left[1] * STICK_TRAVEL).toFixed(1)})`)
    caps.r.setAttribute('transform', `translate(${(s.sticks.right[0] * STICK_TRAVEL).toFixed(1)} ${(-s.sticks.right[1] * STICK_TRAVEL).toFixed(1)})`)
    fills.l2.setAttribute('width', (TRIGGER_W * s.triggers.l2).toFixed(1))
    fills.r2.setAttribute('width', (TRIGGER_W * s.triggers.r2).toFixed(1))
    const key = s.touch.map((t) => `${t.x.toFixed(3)},${t.y.toFixed(3)}`).join(' ')
    if (touches.dataset.key !== key) {
      touches.dataset.key = key
      touches.innerHTML = s.touch
        .map((t) => `<circle class="touch" cx="${(TOUCH.x + t.x * TOUCH.w).toFixed(1)}" cy="${(TOUCH.y + t.y * TOUCH.h).toFixed(1)}" r="9" />`)
        .join('')
    }
    return pressed
  }

  return { draw, setLabels, select }
}
