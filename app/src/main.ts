import { listen } from '@tauri-apps/api/event'
import { mountPad } from './pad'
import { mountChat } from './chat'
import type { PadState } from './types'

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T

const drawPad = mountPad($('pad'))
mountChat()

/** Крестовина оружия из layout-standard.md: □ 1/2, ✕ 3/4, ○ 5/6, △ 7/8. */
const WEAPONS = [
  { face: 'triangle', glyph: '△', slots: [7, 8] },
  { face: 'square', glyph: '□', slots: [1, 2] },
  { face: 'circle', glyph: '○', slots: [5, 6] },
  { face: 'cross', glyph: '✕', slots: [3, 4] },
]
const GLYPHS: Record<string, string> = {
  triangle: '<polygon points="0,-6 5.5,4 -5.5,4" />',
  circle: '<circle r="5.5" />',
  cross: '<path d="M -5 -5 L 5 5 M 5 -5 L -5 5" />',
  square: '<rect x="-5" y="-5" width="10" height="10" />',
}
const weaponsEl = $('weapons')
weaponsEl.innerHTML = WEAPONS.map((w) => `<div class="weapon weapon-${w.face}" data-face="${w.face}">
  <svg class="weapon-glyph" viewBox="-8 -8 16 16">${GLYPHS[w.face]}</svg>
  ${w.slots.map((n) => `<span class="slot" data-slot="${n}">${n}</span>`).join('')}</div>`).join('')
const slotEls = [...weaponsEl.querySelectorAll<HTMLElement>('[data-slot]')]
const faceEls = [...weaponsEl.querySelectorAll<HTMLElement>('[data-face]')]

const GYRO_AXES = ['pitch', 'yaw', 'roll']
const gyroEl = $('gyro')
gyroEl.innerHTML = GYRO_AXES.map((a) => `<div class="gyro-row"><span>${a}</span>
  <div class="bar"><i id="gyro-${a}"></i></div><b id="gyro-${a}-v">0</b></div>`).join('')

const BUS = { usb: 'USB', bluetooth: 'Bluetooth' } as Record<string, string>
let lastKey = ''

function drawStatus(s: PadState) {
  $('pad-info').textContent = `${s.pad.model} · ${BUS[s.pad.bus] ?? s.pad.bus} · батарея ${Math.round(s.pad.battery * 100)}%`
  $('profile').textContent = s.profile ?? '—'

  const layer = $('layer')
  layer.textContent = s.layer ? `слой ${s.layer.toUpperCase()}` : 'базовый'
  layer.classList.toggle('active', Boolean(s.layer))
  weaponsEl.classList.toggle('active', s.layer === 'r2')

  for (const el of slotEls) el.classList.toggle('current', Number(el.dataset.slot) === s.weapons.current)
  for (const el of faceEls) el.classList.toggle('next', el.dataset.face === s.weapons.next_double)
  const next = WEAPONS.find((w) => w.face === s.weapons.next_double)
  $('weapon-hint').textContent = next ? `ещё раз ${next.glyph} → ${next.slots[1]}` : ' '

  // Список выходов меняется редко: перерисовываем, только когда он другой.
  const key = s.outputs.join(' ')
  if (key !== lastKey) {
    lastKey = key
    $('outputs').innerHTML = s.outputs.length
      ? s.outputs.map((o) => `<span class="key">${o}</span>`).join('')
      : '<span class="empty">ничего</span>'
  }

  s.gyro_dps.forEach((v, i) => {
    const a = GYRO_AXES[i]
    const k = Math.max(-1, Math.min(1, v / 180))
    const bar = $(`gyro-${a}`)
    bar.style.left = k < 0 ? `${50 + k * 50}%` : '50%'
    bar.style.width = `${Math.abs(k) * 50}%`
    $(`gyro-${a}-v`).textContent = v.toFixed(0)
  })
}

// Событий ~60 в секунду: рисуем только последнее за кадр.
let pending: PadState | null = null
listen<PadState>('state', (e) => {
  if (!pending) requestAnimationFrame(() => {
    if (pending) { drawPad(pending); drawStatus(pending) }
    pending = null
  })
  pending = e.payload
})
