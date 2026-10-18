import { keyFace } from './keys'
import { TITLE } from './editor'
import { QUARTER_HALF, quarterBound } from './pad'
import type { BindingEntry, Bindings } from './types'

/**
 * Карточка выбранной кнопки: две плитки — что она делает обычно и с зажатым R2. Клавиша крупно,
 * действие рядом, клик по плитке — правка.
 */

const LAYER = 'r2'
const PENCIL = '<svg class="tile-edit" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16v4Z" /><path d="m13.5 6.5 4 4" /></svg>'

function cap(k: string): string {
  const f = keyFace(k)
  return `<span class="cap${f.wide ? ' wide' : ''}" title="${f.title}"><b>${f.main}</b>${f.sub ? `<i>${f.sub}</i>` : ''}</span>`
}

/** Клавиши плитки: первая часть крупно, остальное — подписью под действием. */
function keys(e: BindingEntry): { main: string; more: string } {
  const [first, ...rest] = e.parts
  const main = first.keys.map(cap).join('<span class="plus">+</span>')
  const more = rest
    .map((p) => {
      const ks = p.keys.map((k) => keyFace(k).main).join(' + ')
      return p.how === 'hold' ? `держать — ${ks}` : p.how === 'double' ? `дважды — ${ks}` : p.how === 'next' ? `повторно — ${ks}` : ks
    })
    .concat(first.how === 'latch' ? ['один клик, держится до остановки'] : [])
  return { main, more: more.join(' · ') }
}

function action(e: BindingEntry): string {
  if (e.note) return e.note
  if (e.parts.some((p) => p.how === 'next')) return 'оружие'
  return 'без описания'
}

function tile(label: string, e: BindingEntry | undefined, layer: string | null, base: boolean): string {
  if (!e) {
    return `<button class="tile empty" data-edit="${layer ?? ''}">
      <span class="tile-key"><span class="cap ghost">+</span></span>
      <span class="tile-text"><span class="tile-act">Назначить клавишу</span><span class="tile-meta">${label}</span></span>${PENCIL}</button>`
  }
  const k = keys(e)
  const origin = base ? '' : e.own ? ' · своё' : ' · из стандарта'
  return `<button class="tile${layer ? ' tile-layer' : ''}" data-edit="${layer ?? ''}">
    <span class="tile-key">${k.main}</span>
    <span class="tile-text"><span class="tile-act">${action(e)}</span>
      ${k.more ? `<span class="tile-more">${k.more}</span>` : ''}
      <span class="tile-meta">${label}${origin}</span></span>${PENCIL}</button>`
}

export function renderInspector(el: HTMLElement, b: Bindings | null, id: string | null, onEdit: (id: string, layer: string | null) => void) {
  if (!b || !id) {
    el.innerHTML = `<div class="ins-empty">
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 8h10a4 4 0 0 1 4 4.4l-.5 4a2.6 2.6 0 0 1-4.6 1.3L14.5 16h-5l-1.4 1.7a2.6 2.6 0 0 1-4.6-1.3l-.5-4A4 4 0 0 1 7 8Z" /><path d="M8 11v3M6.5 12.5h3" /></svg>
      <p>Нажми кнопку на геймпаде или наведи на неё — здесь будет, что она делает в игре.</p>
      <p class="faint-p">Клик по кнопке на схеме — поменять клавишу.</p></div>`
    return
  }
  const base = b.file.startsWith('_')
  const title = `<div class="ins-title">${TITLE[id] ?? id}</div>`
  if (id === LAYER && !b.buttons[id]) {
    el.innerHTML = `${title}<p class="ins-note">Кнопка второго слоя. Пока держишь R2, остальные кнопки дают клавиши слоя — на схеме они розовые.</p>`
    return
  }
  if (id === 'ps' && !b.buttons[id]) {
    el.innerHTML = `${title}<p class="ins-note">Калибровка гироскопа: тап — начать и закончить, удержание — калибрует, пока держишь. Положи геймпад неподвижно.</p><p class="ins-note">PS + Create — пауза.</p>`
    return
  }
  // четверть тачпада без своей привязки работает как её половина
  const half = QUARTER_HALF[id]
  const src = half && !quarterBound(b, id) ? half : id
  const shared = src !== id ? ' · как вся половина' : ''
  el.innerHTML = `${title}
    ${tile('Обычно' + shared, b.buttons[src], null, base)}
    ${tile('С зажатым R2' + shared, b.layers[LAYER]?.[src], LAYER, base)}`
  for (const btn of el.querySelectorAll<HTMLElement>('[data-edit]')) {
    btn.addEventListener('click', () => onEdit(id, btn.dataset.edit || null))
  }
}
