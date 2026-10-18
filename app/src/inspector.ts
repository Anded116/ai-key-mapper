import { keyFace } from './keys'
import { TITLE } from './editor'
import type { BindingEntry, Bindings } from './types'

/** Карточка выбранной кнопки: что она делает обычно и с R2, откуда привязка, кнопка правки. */

const LAYER = 'r2'

function caps(e: BindingEntry): string {
  return e.parts
    .map((part, i) => {
      const keys = part.keys
        .map((k) => {
          const f = keyFace(k)
          return `<span class="cap${f.wide ? ' wide' : ''}" title="${f.title}"><b>${f.main}</b>${f.sub ? `<i>${f.sub}</i>` : ''}</span>`
        })
        .join('<span class="plus">+</span>')
      const sep = i === 0 ? '' : part.how === 'next' ? '<span class="sep">потом</span>' : ''
      const how = part.how === 'hold' ? '<span class="how">держать</span>' : part.how === 'double' ? '<span class="how">дважды</span>' : part.how === 'tap' && e.parts.length > 1 ? '<span class="how">нажать</span>' : ''
      const after = part.how === 'latch' ? '<span class="how">клик, до остановки</span>' : ''
      return `${sep}${how}${keys}${after}`
    })
    .join('')
}

function block(title: string, e: BindingEntry | undefined, layer: string | null, base: boolean): string {
  const origin = !e ? '' : base ? '' : e.own ? '<span class="origin own">своё</span>' : '<span class="origin">из стандарта</span>'
  const body = e
    ? `<div class="ins-act">${e.note ?? (e.parts.some((p) => p.how === 'next') ? 'оружие: первое нажатие и повторное' : '<span class="faint">без описания</span>')}</div><div class="ins-caps">${caps(e)}</div>`
    : '<div class="ins-act faint">не назначено</div>'
  return `<div class="ins-block">
    <div class="ins-head"><span>${title}</span>${origin}<button class="link" data-edit="${layer ?? ''}">${e ? 'Изменить' : 'Назначить'}</button></div>
    ${body}</div>`
}

export function renderInspector(el: HTMLElement, b: Bindings | null, id: string | null, onEdit: (id: string, layer: string | null) => void) {
  if (!b || !id) {
    el.innerHTML = `<div class="ins-empty">Наведи на кнопку или нажми её на геймпаде — здесь будет, что она делает. Клик по кнопке — поменять.</div>`
    return
  }
  const base = b.file.startsWith('_')
  if (id === LAYER && !b.buttons[id]) {
    el.innerHTML = `<div class="ins-title">${TITLE[id]}</div><div class="ins-act">Держи — второй слой: на кнопках появятся его клавиши.</div>`
    return
  }
  if (id === 'ps' && !b.buttons[id]) {
    el.innerHTML = `<div class="ins-title">PS</div><div class="ins-act">Калибровка гироскопа: тап — начать и закончить, удержание — пока держишь. PS + Create — пауза.</div>`
    return
  }
  const layered = b.layers[LAYER]?.[id]
  el.innerHTML = `<div class="ins-title">${TITLE[id] ?? id}</div>
    ${block('Обычно', b.buttons[id], null, base)}
    ${block('С зажатым R2', layered, LAYER, base)}`
  for (const btn of el.querySelectorAll<HTMLElement>('[data-edit]')) {
    btn.addEventListener('click', () => onEdit(id, btn.dataset.edit || null))
  }
}
