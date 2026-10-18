import { keyFace } from './keys'
import type { BindingEntry, Bindings, PadState } from './types'

/**
 * Шпаргалка вокруг схемы: у каждой кнопки — что она делает в игре и какая это клавиша, крупно
 * по-русски, как её пишет игра. Вторая строка — та же кнопка с зажатым R2. Строки стоят как
 * можно ближе к своим кнопкам, тонкие выноски ведут к схеме.
 */

const LEFT = ['l2', 'l1', 'create', 'pad_left', 'up', 'left', 'right', 'down', 'move', 'l3']
const RIGHT = ['r2', 'r1', 'options', 'pad_right', 'triangle', 'square', 'circle', 'cross', 'r3']
const BOTTOM = ['mic', 'ps']
const LAYER = 'r2'
const GAP = 6

type Side = 'left' | 'right' | 'bottom'

interface Pad {
  anchorAt(id: string): DOMPoint | null
  focus(id: string | null): void
}

function cap(name: string, extra = ''): string {
  const f = keyFace(name)
  const sub = f.sub ? `<i>${f.sub}</i>` : ''
  return `<span class="cap${f.wide ? ' wide' : ''}${extra}" data-key="${name}" title="${f.title}"><b>${f.main}</b>${sub}</span>`
}

function caps(e: BindingEntry): string {
  return e.parts
    .map((part, i) => {
      const keys = part.keys.map((k) => cap(k)).join('<span class="plus">+</span>')
      const sep = i === 0 || e.parts.every((p) => p.how === 'press') ? '' : part.how === 'next' ? '<span class="sep">/</span>' : '<span class="sep">·</span>'
      const how = part.how === 'hold' ? '<span class="how">держать</span>' : part.how === 'double' ? '<span class="how">×2</span>' : ''
      const after = part.how === 'latch' ? '<span class="how">клик</span>' : ''
      return `${sep}${how}${keys}${after}`
    })
    .join('')
}

/** Строка: описание и клавиши; клавиши всегда ближе к схеме, слева и справа зеркально. */
function line(e: BindingEntry | undefined, cls: string, side: Side, tag = ''): string {
  const act = e ? (e.note ?? '') : 'не назначено'
  const actEl = `<span class="act" title="${act}">${act}</span>`
  const capsEl = `<span class="caps">${e ? caps(e) : '<span class="cap ghost">+</span>'}</span>`
  const inner = side === 'left' ? actEl + tag + capsEl : capsEl + tag + actEl
  return `<div class="line ${cls}${e ? '' : ' empty'}">${inner}</div>`
}

export function mountSheet(root: HTMLElement, pad: Pad, onEdit: (id: string, layer: string | null) => void) {
  root.insertAdjacentHTML(
    'beforeend',
    `<div class="col col-left"></div><div class="col col-right"></div><div class="col col-bottom"></div>
     <svg class="leaders" xmlns="http://www.w3.org/2000/svg"></svg>`,
  )
  const cols: Record<Side, HTMLElement> = {
    left: root.querySelector('.col-left')!,
    right: root.querySelector('.col-right')!,
    bottom: root.querySelector('.col-bottom')!,
  }
  const leaders = root.querySelector<SVGSVGElement>('.leaders')!
  let bindings: Bindings | null = null
  let rows: Record<string, HTMLElement> = {}
  let wasd = false

  const render = () => {
    rows = {}
    for (const [side, ids] of [['left', LEFT], ['right', RIGHT], ['bottom', BOTTOM]] as [Side, string[]][]) {
      cols[side].innerHTML = ids
        .map((id) => {
          if (!bindings) return ''
          if (id === 'move') {
            const move: BindingEntry = {
              parts: bindings.move_keys.map((k) => ({ how: 'press' as const, keys: [k] })),
              note: wasd ? 'ходьба' : 'ходьба: стик читает игра',
              own: false,
            }
            return `<div class="row move${wasd ? '' : ' off'}" data-id="move">${line(move, 'base', side)}</div>`
          }
          const base = bindings.buttons[id]
          const layered = bindings.layers[LAYER]?.[id]
          const fixed = id === 'ps' && !base ? 'калибровка гироскопа: тап / держать' : id === LAYER && !base ? 'держать — второй слой (строки R2)' : ''
          if (fixed) return `<div class="row static" data-id="${id}"><div class="line base"><span class="act">${fixed}</span></div></div>`
          const tag = `<span class="tag">R2</span>`
          const second = layered ? line(layered, 'layer', side, tag) : ''
          return `<div class="row" data-id="${id}">${line(base, 'base', side)}${second}</div>`
        })
        .join('')
      for (const el of cols[side].querySelectorAll<HTMLElement>('.row')) rows[el.dataset.id!] = el
    }
    for (const [id, el] of Object.entries(rows)) {
      el.addEventListener('mouseenter', () => hover(id, true))
      el.addEventListener('mouseleave', () => hover(id, false))
      if (id === 'move' || el.classList.contains('static')) continue
      el.querySelector('.line.base')?.addEventListener('click', () => onEdit(id, null))
      el.querySelector('.line.layer')?.addEventListener('click', () => onEdit(id, LAYER))
    }
    layout()
  }

  const hover = (id: string, on: boolean) => {
    pad.focus(on ? id : null)
    leaders.querySelector(`[data-id="${id}"]`)?.classList.toggle('focus', on)
  }

  /** Строки — к высоте своих кнопок, без наложений; затем выноски. */
  const layout = () => {
    const box = root.getBoundingClientRect()
    const paths: string[] = []
    for (const side of ['left', 'right'] as const) {
      const items = [...cols[side].querySelectorAll<HTMLElement>('.row')]
        .map((el) => {
          const a = pad.anchorAt(el.dataset.id!)
          return { el, a, h: el.offsetHeight, top: 0 }
        })
        .filter((r) => r.a)
        .sort((x, y) => x.a!.y - y.a!.y)
      const height = cols[side].clientHeight
      let floor = 0
      for (const r of items) {
        const want = r.a!.y - box.top - Math.min(r.h, 30) / 2
        r.top = Math.max(want, floor)
        floor = r.top + r.h + GAP
      }
      let ceil = height
      for (const r of [...items].reverse()) {
        r.top = Math.max(0, Math.min(r.top, ceil - r.h))
        ceil = r.top - GAP
      }
      for (const r of items) {
        r.el.style.top = `${r.top}px`
        const capsEl = r.el.querySelector('.line.base .caps') ?? r.el.querySelector('.line')!
        const c = capsEl.getBoundingClientRect()
        const y = c.top + c.height / 2 - box.top
        const x = side === 'left' ? c.right - box.left + 6 : c.left - box.left - 6
        const ax = r.a!.x - box.left
        const ay = r.a!.y - box.top
        const bend = side === 'left' ? x + 14 : x - 14
        paths.push(`<g data-id="${r.el.dataset.id}"><path d="M ${x} ${y} H ${bend} L ${ax} ${ay}" /><circle cx="${ax}" cy="${ay}" r="2.2" /></g>`)
      }
    }
    for (const el of cols.bottom.querySelectorAll<HTMLElement>('.row')) {
      const a = pad.anchorAt(el.dataset.id!)
      if (!a) continue
      const c = el.getBoundingClientRect()
      const x = c.left + c.width / 2 - box.left
      paths.push(`<g data-id="${el.dataset.id}"><path d="M ${x} ${c.top - box.top - 4} L ${a.x - box.left} ${a.y - box.top}" /><circle cx="${a.x - box.left}" cy="${a.y - box.top}" r="2.2" /></g>`)
    }
    leaders.innerHTML = paths.join('')
  }

  new ResizeObserver(() => layout()).observe(root)

  return {
    setBindings(b: Bindings | null) {
      bindings = b
      render()
    },
    draw(s: PadState) {
      const isWasd = s.engine.movement === 'wasd'
      if (isWasd !== wasd) {
        wasd = isWasd
        render()
      }
      root.classList.toggle('in-layer', s.layer === LAYER)
      const pressed = new Set(s.buttons.map((b) => b.toLowerCase()))
      if (pressed.has('touchpad') && s.touch.length) pressed.add(s.touch[0].x < 0.5 ? 'pad_left' : 'pad_right')
      for (const [id, el] of Object.entries(rows)) {
        const on = id === 'move' ? Math.hypot(...s.sticks.left) > 0.3 : pressed.has(id)
        if (el.classList.contains('on') !== on) {
          el.classList.toggle('on', on)
          leaders.querySelector(`[data-id="${id}"]`)?.classList.toggle('on', on)
        }
      }
      const current = s.weapons.current === null ? '' : String(s.weapons.current)
      if (root.dataset.weapon !== current) {
        root.dataset.weapon = current
        for (const c of root.querySelectorAll<HTMLElement>('.line.layer .cap')) c.classList.toggle('current', c.dataset.key === current)
      }
    },
    layout,
  }
}
