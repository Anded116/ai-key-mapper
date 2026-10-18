import type { PadState } from './types'

/** Схема DualSense: SVG с элементами `data-btn`, которые подсвечиваются по `state.buttons`. */

const DPAD = { x: 175, y: 205 }
const FACE = { x: 625, y: 205 }
const STICK_L = { x: 290, y: 305 }
const STICK_R = { x: 510, y: 305 }
const STICK_TRAVEL = 24
const TOUCH = { x: 290, y: 98, w: 220, h: 126 }
const TRIGGER_W = 120

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
  const r = 21
  const at = { triangle: [0, -45], circle: [45, 0], cross: [0, 45], square: [-45, 0] } as const
  const glyph = {
    triangle: '<polygon points="0,-9 8.5,6 -8.5,6" />',
    circle: '<circle r="8" />',
    cross: '<path d="M -7 -7 L 7 7 M 7 -7 L -7 7" />',
    square: '<rect x="-7" y="-7" width="14" height="14" />',
  }
  return (Object.keys(at) as (keyof typeof at)[])
    .map((b) => `<g transform="translate(${FACE.x + at[b][0]} ${FACE.y + at[b][1]})">
      <circle class="btn" data-btn="${b}" r="${r}" />
      <g class="glyph glyph-${b}">${glyph[b]}</g></g>`)
    .join('')
}

function stick(id: 'l' | 'r', c: { x: number; y: number }): string {
  return `<circle class="stick-base" cx="${c.x}" cy="${c.y}" r="44" />
    <line class="stick-ray" id="ray-${id}" x1="${c.x}" y1="${c.y}" x2="${c.x}" y2="${c.y}" />
    <g id="stick-${id}"><circle class="btn stick-cap" data-btn="${id}3" cx="${c.x}" cy="${c.y}" r="28" />
    <text class="label" x="${c.x}" y="${c.y + 4}">${id.toUpperCase()}3</text></g>`
}

function trigger(id: 'l2' | 'r2', x: number): string {
  return `<g><rect class="btn trigger" data-btn="${id}" x="${x}" y="20" width="${TRIGGER_W}" height="34" rx="12" />
    <rect class="trigger-fill" id="fill-${id}" x="${x}" y="20" width="0" height="34" rx="12" />
    <text class="label" x="${x + TRIGGER_W / 2}" y="42">${id.toUpperCase()}</text></g>`
}

export function mountPad(root: HTMLElement): (s: PadState) => void {
  root.innerHTML = `<svg viewBox="0 0 800 520" class="pad" xmlns="http://www.w3.org/2000/svg">
    ${trigger('l2', 150)}${trigger('r2', 530)}
    <rect class="btn" data-btn="l1" x="145" y="64" width="130" height="22" rx="11" />
    <text class="label" x="210" y="80">L1</text>
    <rect class="btn" data-btn="r1" x="525" y="64" width="130" height="22" rx="11" />
    <text class="label" x="590" y="80">R1</text>
    <path class="body" d="${BODY}" />
    <path class="lightbar" id="lightbar-l" d="M ${TOUCH.x - 7} ${TOUCH.y + 14} L ${TOUCH.x - 7} ${TOUCH.y + TOUCH.h - 8}" />
    <path class="lightbar" id="lightbar-r" d="M ${TOUCH.x + TOUCH.w + 7} ${TOUCH.y + 14} L ${TOUCH.x + TOUCH.w + 7} ${TOUCH.y + TOUCH.h - 8}" />
    <rect class="btn touchpad" data-btn="touchpad" x="${TOUCH.x}" y="${TOUCH.y}" width="${TOUCH.w}" height="${TOUCH.h}" rx="16" />
    <g id="touches"></g>
    <rect class="btn" data-btn="create" x="250" y="116" width="14" height="32" rx="7" />
    <rect class="btn" data-btn="options" x="536" y="116" width="14" height="32" rx="7" />
    <circle class="btn" data-btn="ps" cx="400" cy="280" r="14" />
    <rect class="btn" data-btn="mic" x="384" y="306" width="32" height="9" rx="4.5" />
    ${dpad()}${face()}${stick('l', STICK_L)}${stick('r', STICK_R)}
  </svg>`

  const svg = root.querySelector('svg')!
  const buttons = [...svg.querySelectorAll<SVGElement>('[data-btn]')]
  const touches = svg.querySelector<SVGGElement>('#touches')!
  const caps = { l: svg.querySelector('#stick-l')!, r: svg.querySelector('#stick-r')! }
  const rays = { l: svg.querySelector('#ray-l')!, r: svg.querySelector('#ray-r')! }
  const fills = { l2: svg.querySelector('#fill-l2')!, r2: svg.querySelector('#fill-r2')! }

  const moveStick = (id: 'l' | 'r', c: { x: number; y: number }, [x, y]: [number, number]) => {
    const dx = x * STICK_TRAVEL
    const dy = -y * STICK_TRAVEL
    caps[id].setAttribute('transform', `translate(${dx.toFixed(1)} ${dy.toFixed(1)})`)
    rays[id].setAttribute('x2', (c.x + dx * 1.8).toFixed(1))
    rays[id].setAttribute('y2', (c.y + dy * 1.8).toFixed(1))
  }

  return (s) => {
    const pressed = new Set(s.buttons.map((b) => b.toLowerCase()))
    for (const el of buttons) el.classList.toggle('on', pressed.has(el.dataset.btn!))
    moveStick('l', STICK_L, s.sticks.left)
    moveStick('r', STICK_R, s.sticks.right)
    fills.l2.setAttribute('width', (TRIGGER_W * s.triggers.l2).toFixed(1))
    fills.r2.setAttribute('width', (TRIGGER_W * s.triggers.r2).toFixed(1))
    touches.innerHTML = s.touch
      .map((t) => `<circle class="touch" cx="${(TOUCH.x + t.x * TOUCH.w).toFixed(1)}" cy="${(TOUCH.y + t.y * TOUCH.h).toFixed(1)}" r="9" />`)
      .join('')
    svg.classList.toggle('layer-r2', s.layer === 'r2')
  }
}
