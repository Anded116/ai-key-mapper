import type { PadState } from './types'

/** Схема DualSense: контуры кнопок с `data-btn`, живые стики, курки и касания тачпада. */

const DPAD = { x: 175, y: 205 }
const FACE = { x: 625, y: 205 }
const STICK_L = { x: 290, y: 305 }
const STICK_R = { x: 510, y: 305 }
const STICK_TRAVEL = 22
const TOUCH = { x: 290, y: 98, w: 220, h: 126 }
const TRIGGER_W = 120

/** Куда в схеме упирается выноска каждой кнопки. */
export const ANCHORS: Record<string, [number, number]> = {
  l2: [210, 37], r2: [590, 37], l1: [210, 75], r1: [590, 75],
  create: [257, 132], options: [543, 132], pad_left: [340, 175], pad_right: [460, 175],
  up: [175, 172], left: [142, 205], right: [208, 205], down: [175, 238],
  triangle: [625, 160], square: [580, 205], circle: [670, 205], cross: [625, 250],
  move: [262, 284], l3: [290, 305], r3: [510, 305], ps: [400, 280], mic: [400, 311],
}

const BODY = `M 230 100 C 330 88, 470 88, 570 100 C 650 108, 700 130, 720 190 C 745 270, 770 380, 760 440
  C 752 490, 690 500, 660 470 C 630 440, 600 380, 560 352 C 520 332, 280 332, 240 352
  C 200 380, 170 440, 140 470 C 110 500, 48 490, 40 440 C 30 380, 55 270, 80 190 C 100 130, 150 108, 230 100 Z`

function dpad(): string {
  const arrow = '-15,-48 15,-48 15,-21 0,-7 -15,-21'
  return (['up', 'right', 'down', 'left'] as const)
    .map((b, i) => `<polygon class="btn" data-btn="${b}" points="${arrow}"
      transform="translate(${DPAD.x} ${DPAD.y}) rotate(${i * 90})" />`)
    .join('')
}

function face(): string {
  const at = { triangle: [0, -45], circle: [45, 0], cross: [0, 45], square: [-45, 0] } as const
  const glyph = {
    triangle: '<polygon points="0,-8 7.5,5.5 -7.5,5.5" />',
    circle: '<circle r="7" />',
    cross: '<path d="M -6 -6 L 6 6 M 6 -6 L -6 6" />',
    square: '<rect x="-6" y="-6" width="12" height="12" />',
  }
  return (Object.keys(at) as (keyof typeof at)[])
    .map((b) => `<g transform="translate(${FACE.x + at[b][0]} ${FACE.y + at[b][1]})">
      <circle class="btn" data-btn="${b}" r="20" />
      <g class="glyph glyph-${b}">${glyph[b]}</g></g>`)
    .join('')
}

function stick(id: 'l' | 'r', c: { x: number; y: number }): string {
  return `<circle class="stick-base" cx="${c.x}" cy="${c.y}" r="44" />
    <g id="stick-${id}"><circle class="btn stick-cap" data-btn="${id}3" cx="${c.x}" cy="${c.y}" r="27" />
    <circle class="stick-dot" cx="${c.x}" cy="${c.y}" r="3" /></g>`
}

function trigger(id: 'l2' | 'r2', x: number): string {
  return `<rect class="btn trigger" data-btn="${id}" x="${x}" y="22" width="${TRIGGER_W}" height="30" rx="11" />
    <rect class="trigger-fill" id="fill-${id}" x="${x}" y="22" width="0" height="30" rx="11" />`
}

export function mountPad(root: HTMLElement) {
  root.innerHTML = `<svg viewBox="20 10 760 500" class="pad" xmlns="http://www.w3.org/2000/svg">
    ${trigger('l2', 150)}${trigger('r2', 530)}
    <rect class="btn" data-btn="l1" x="150" y="64" width="120" height="20" rx="10" />
    <rect class="btn" data-btn="r1" x="530" y="64" width="120" height="20" rx="10" />
    <path class="body" d="${BODY}" />
    <rect class="btn touchpad" data-btn="touchpad" x="${TOUCH.x}" y="${TOUCH.y}" width="${TOUCH.w}" height="${TOUCH.h}" rx="16" />
    <line class="touch-split" x1="400" y1="${TOUCH.y + 18}" x2="400" y2="${TOUCH.y + TOUCH.h - 18}" />
    <g id="touches"></g>
    <rect class="btn" data-btn="create" x="250" y="116" width="14" height="32" rx="7" />
    <rect class="btn" data-btn="options" x="536" y="116" width="14" height="32" rx="7" />
    <circle class="btn" data-btn="ps" cx="400" cy="280" r="13" />
    <rect class="btn" data-btn="mic" x="385" y="306" width="30" height="9" rx="4.5" />
    ${dpad()}${face()}${stick('l', STICK_L)}${stick('r', STICK_R)}
    <g id="anchors"></g>
  </svg>`

  const svg = root.querySelector('svg')!
  const buttons = [...svg.querySelectorAll<SVGElement>('[data-btn]')]
  const touches = svg.querySelector<SVGGElement>('#touches')!
  const caps = { l: svg.querySelector('#stick-l')!, r: svg.querySelector('#stick-r')! }
  const fills = { l2: svg.querySelector('#fill-l2')!, r2: svg.querySelector('#fill-r2')! }

  const moveStick = (id: 'l' | 'r', [x, y]: [number, number]) => {
    caps[id].setAttribute('transform', `translate(${(x * STICK_TRAVEL).toFixed(1)} ${(-y * STICK_TRAVEL).toFixed(1)})`)
  }

  /** Точка привязки выноски в координатах страницы. */
  const anchorAt = (id: string): DOMPoint | null => {
    const a = ANCHORS[id]
    const m = svg.getScreenCTM()
    if (!a || !m) return null
    return new DOMPoint(a[0], a[1]).matrixTransform(m)
  }

  /** Подсветить кнопку, на чью строку навели мышь. */
  const focus = (id: string | null) => {
    const target = id === 'move' ? 'l3' : id === 'pad_left' || id === 'pad_right' ? 'touchpad' : id
    for (const el of buttons) el.classList.toggle('focus', el.dataset.btn === target)
  }

  const draw = (s: PadState) => {
    const pressed = new Set(s.buttons.map((b) => b.toLowerCase()))
    for (const el of buttons) el.classList.toggle('on', pressed.has(el.dataset.btn!))
    moveStick('l', s.sticks.left)
    moveStick('r', s.sticks.right)
    fills.l2.setAttribute('width', (TRIGGER_W * s.triggers.l2).toFixed(1))
    fills.r2.setAttribute('width', (TRIGGER_W * s.triggers.r2).toFixed(1))
    const key = s.touch.map((t) => `${t.x.toFixed(3)},${t.y.toFixed(3)}`).join(' ')
    if (touches.dataset.key !== key) {
      touches.dataset.key = key
      touches.innerHTML = s.touch
        .map((t) => `<circle class="touch" cx="${(TOUCH.x + t.x * TOUCH.w).toFixed(1)}" cy="${(TOUCH.y + t.y * TOUCH.h).toFixed(1)}" r="8" />`)
        .join('')
    }
    svg.classList.toggle('layer-r2', s.layer === 'r2')
  }

  return { svg, draw, anchorAt, focus }
}
