import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { mountPad } from './pad'
import { mountSheet } from './sheet'
import { mountEditor } from './editor'
import { mountChat } from './chat'
import { keyFace } from './keys'
import type { Bindings, PadState } from './types'

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T

let bindings: Bindings | null = null
const pad = mountPad($('pad'))
const editor = mountEditor($('editor'), () => bindings)
const sheet = mountSheet($('sheet'), pad, (id, layer) => editor.open(id, layer))
mountChat()

const showBindings = (b: Bindings | null) => {
  bindings = b
  $('sheet').classList.toggle('has-profile', Boolean(b))
  sheet.setBindings(b)
}
listen<Bindings | null>('bindings', ({ payload }) => showBindings(payload))
  .then(() => invoke<Bindings | null>('bindings'))
  .then(showBindings)

// Чат — шторка справа; открыта ли она, окно помнит.
const CHAT_KEY = 'aikm.chat-open'
const setChat = (open: boolean) => {
  $('app').classList.toggle('chat-open', open)
  $('chat-toggle').classList.toggle('active', open)
  try { localStorage.setItem(CHAT_KEY, open ? '1' : '0') } catch { /* окно без хранилища */ }
}
let chatOpen = true
try { chatOpen = localStorage.getItem(CHAT_KEY) !== '0' } catch { /* окно без хранилища */ }
setChat(chatOpen)
$('chat-toggle').addEventListener('click', () => setChat(!$('app').classList.contains('chat-open')))

const BUS: Record<string, string> = { usb: 'USB', bluetooth: 'Bluetooth' }
const MOVEMENT: Record<string, string> = { wasd: 'Стик → WASD', hybrid: 'Стик читает игра' }
let paused = false
const cache: Record<string, string> = {}

/** Записать в элемент, только если поменялось: 60 кадров в секунду не должны трогать DOM зря. */
const put = (id: string, value: string, html = false) => {
  if (cache[id] === value) return
  cache[id] = value
  if (html) $(id).innerHTML = value
  else $(id).textContent = value
}

function drawTop(s: PadState) {
  const e = s.engine
  paused = e.paused
  const [state, tone] = !s.pad
    ? ['геймпад не найден', 'off']
    : !s.profile
      ? ['жду игру с профилем', 'off']
      : e.paused
        ? ['пауза', 'warn']
        : e.focused
          ? ['работает', 'ok']
          : ['игра не в фокусе', 'off']
  put('profile', s.profile ?? 'Нет профиля')
  put('engine-state', state)
  $('status-dot').dataset.tone = tone

  const tags = [e.movement ? MOVEMENT[e.movement] ?? e.movement : '', e.hidden ? 'Геймпад скрыт от игры' : '', e.calibrating ? 'Калибровка гироскопа' : '']
  put('tags', tags.filter(Boolean).map((t) => `<span class="tag-pill">${t}</span>`).join(''), true)

  const p = s.pad
  put('device-name', p ? `${p.model} · ${BUS[p.bus] ?? p.bus}` : 'геймпад не найден')
  const battery = $('battery')
  battery.hidden = !p || p.battery === null
  if (p && p.battery !== null) {
    const pct = Math.round(p.battery * 100)
    put('battery-text', p.charging ? `${pct}% · заряжается` : `${pct}%`)
    $('battery-fill').setAttribute('width', (18 * p.battery).toFixed(1))
    battery.classList.toggle('charging', p.charging)
    battery.classList.toggle('low', !p.charging && pct <= 15)
  }
  $('pause').classList.toggle('active', e.paused)
  $('pause').title = e.paused ? 'Продолжить' : 'Пауза — то же, что PS + Create'
  $('calibrate').classList.toggle('active', e.calibrating)
  $('calibrate').toggleAttribute('disabled', !p)

  put('outputs', s.outputs.map((o) => `<span class="mini-cap">${keyFace(o).main}</span>`).join('') || '<span class="faint">ничего</span>', true)
}

$('pause').addEventListener('click', () => invoke('set_paused', { paused: !paused }))
$('calibrate').addEventListener('click', () => invoke('calibrate'))

// Журнал движка: в подвале последняя строка, по клику — последние десять.
const logLines: string[] = []
const showLog = () => {
  const last = logLines[logLines.length - 1] ?? ''
  $('logline').textContent = last
  $('log').innerHTML = logLines.slice(-10).reverse().map((l) => `<li>${l.replace(/</g, '&lt;')}</li>`).join('')
}
$('logline').addEventListener('click', () => { $('log').hidden = !$('log').hidden })
listen<string>('log', ({ payload }) => {
  logLines.push(payload)
  showLog()
}).then(() => invoke<string[]>('engine_log')).then((history) => {
  logLines.splice(0, logLines.length, ...history)
  showLog()
})

// Событий ~60 в секунду: рисуем только последнее за кадр.
let pending: PadState | null = null
listen<PadState>('state', (e) => {
  if (!pending) requestAnimationFrame(() => {
    if (pending) { pad.draw(pending); sheet.draw(pending); drawTop(pending) }
    pending = null
  })
  pending = e.payload
})
