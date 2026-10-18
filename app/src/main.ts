import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { mountPad } from './pad'
import { mountEditor } from './editor'
import { renderInspector } from './inspector'
import { mountChat } from './chat'
import { keyFace, setPromptLang } from './keys'
import type { Bindings, PadState } from './types'

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T

interface ProfileInfo {
  file: string
  name: string
  exe: string | null
  base: boolean
}

/** Что показано: какой профиль, какой слой, какая кнопка. */
let profiles: ProfileInfo[] = []
let viewed: Bindings | null = null
let activeFile: string | null = null
let pinnedLayer: string | null = null
let heldLayer: string | null = null
let wasd = false
let hovered: string | null = null
let selected: string | null = null
let lastPressed = new Set<string>()

const editor = mountEditor($('editor'), () => viewed, () => void refresh())
const pad = mountPad(
  $('pad'),
  (id) => { hovered = id; inspect() },
  (id) => { selected = id; editor.open(id, view()) },
)
mountChat()

const view = () => heldLayer ?? pinnedLayer

function inspect() {
  const id = hovered ?? selected
  pad.select(id)
  renderInspector($('inspect'), viewed, id, (b, layer) => editor.open(b, layer))
}

function relabel() {
  pad.setLabels(viewed, view(), wasd)
  for (const btn of $('layer-switch').querySelectorAll<HTMLElement>('button')) {
    btn.classList.toggle('active', (btn.dataset.view || null) === view())
  }
  $('stage').classList.toggle('layer-view', Boolean(view()))
}

function drawProfiles() {
  $('profiles').innerHTML = profiles
    .map((p) => {
      const on = viewed?.file === p.file
      const live = p.file === activeFile
      return `<li><button class="${on ? 'active' : ''}" data-file="${p.file}" title="${p.file}">
        <span class="p-name">${p.name}</span>${live ? '<span class="p-live">в игре</span>' : p.base ? '<span class="p-tag">для всех игр</span>' : ''}</button></li>`
    })
    .join('')
}

async function show(file: string | null) {
  viewed = file ? await invoke<Bindings>('profile_bindings', { file }).catch(() => null) : null
  $('stage-note').textContent = viewed
    ? viewed.file.startsWith('_')
      ? 'Стандарт: игры его наследуют и меняют только своё. Правка здесь меняет все игры, где кнопка не переопределена.'
      : ''
    : 'Выбери профиль слева.'
  setPromptLang(viewed?.prompts ?? 'ru')
  for (const btn of $('prompts').querySelectorAll<HTMLElement>('button')) {
    btn.classList.toggle('active', btn.dataset.lang === (viewed?.prompts ?? 'ru'))
  }
  $('prompts').hidden = !viewed
  drawProfiles()
  relabel()
  inspect()
}

async function refresh() {
  profiles = await invoke<ProfileInfo[]>('profiles').catch(() => [])
  await show(viewed?.file ?? activeFile ?? profiles[0]?.file ?? null)
}

$('profiles').addEventListener('click', (ev) => {
  const btn = (ev.target as HTMLElement).closest<HTMLElement>('[data-file]')
  if (btn) void show(btn.dataset.file!)
})
$('prompts').addEventListener('click', async (ev) => {
  const btn = (ev.target as HTMLElement).closest<HTMLElement>('[data-lang]')
  if (!btn || !viewed || btn.dataset.lang === viewed.prompts) return
  await invoke('set_prompts', { file: viewed.file, lang: btn.dataset.lang })
  await show(viewed.file)
})
$('layer-switch').addEventListener('click', (ev) => {
  const btn = (ev.target as HTMLElement).closest<HTMLElement>('button')
  if (!btn) return
  pinnedLayer = btn.dataset.view || null
  relabel()
})

// Профиль игры в фокусе: показываем его сам, а правки из файла приходят тем же событием.
listen<Bindings | null>('bindings', ({ payload }) => {
  const file = payload?.file ?? null
  const switched = file !== activeFile
  activeFile = file
  if (file && (switched || viewed?.file === file)) void refresh().then(() => show(file))
  else drawProfiles()
})
  .then(() => invoke<Bindings | null>('bindings'))
  .then((b) => {
    activeFile = b?.file ?? null
    return refresh()
  })

// Чат — шторка справа; открыта ли она, окно помнит.
const CHAT_KEY = 'aikm.chat-open'
const setChat = (open: boolean) => {
  $('app').classList.toggle('chat-open', open)
  $('chat-toggle').classList.toggle('active', open)
  try { localStorage.setItem(CHAT_KEY, open ? '1' : '0') } catch { /* окно без хранилища */ }
}
let chatOpen = false
try { chatOpen = localStorage.getItem(CHAT_KEY) === '1' } catch { /* окно без хранилища */ }
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
  put('profile', s.profile ?? 'Нет игры')
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

function drawPad(s: PadState) {
  const held = s.layer
  const isWasd = s.engine.movement === 'wasd'
  if (held !== heldLayer || isWasd !== wasd) {
    heldLayer = held
    wasd = isWasd
    relabel()
  }
  const pressed = pad.draw(s)
  // нажал кнопку на геймпаде — карточка показывает её
  for (const id of pressed) {
    if (!lastPressed.has(id) && id !== 'touchpad' && id !== heldLayer) {
      selected = id
      inspect()
    }
  }
  lastPressed = pressed
}

$('pause').addEventListener('click', () => invoke('set_paused', { paused: !paused }))
$('calibrate').addEventListener('click', () => invoke('calibrate'))

// Журнал движка: в подвале последняя строка, по клику — последние десять.
const logLines: string[] = []
const showLog = () => {
  $('logline').textContent = logLines[logLines.length - 1] ?? ''
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
    if (pending) { drawPad(pending); drawTop(pending) }
    pending = null
  })
  pending = e.payload
})
