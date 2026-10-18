import { invoke } from '@tauri-apps/api/core'
import type { Bindings, PadState } from './types'

/**
 * Проверка поворота: движок крутит камеру ровно на 360° по калибровке профиля, пользователь
 * говорит, насколько она повернулась на самом деле, — `real_world` пересчитывается и пишется в профиль.
 */

type Step = 'idle' | 'armed' | 'turning' | 'ask' | 'done'

const ANGLES: [string, number][] = [
  ['¼', 90], ['½', 180], ['¾', 270], ['1¼', 450], ['1½', 540], ['2', 720],
]

export function mountTurn(el: HTMLElement, getViewed: () => Bindings | null, isActive: () => boolean, onSaved: () => void) {
  let step: Step = 'idle'
  let wasTurning = false
  let message = ''

  const render = () => {
    const b = getViewed()
    if (!b || b.file.startsWith('_')) {
      el.innerHTML = '<p class="side-hint">Калибровка поворота — у каждой игры своя: выбери профиль игры.</p>'
      return
    }
    const value = `<div class="turn-value"><span>real_world</span><b>${+b.real_world.toFixed(4)}</b>${b.in_game_sens !== 1 ? `<span>/ чувств. ${b.in_game_sens}</span>` : ''}</div>`
    if (step === 'armed' || step === 'turning') {
      el.innerHTML = `${value}<div class="turn-box">${step === 'armed'
        ? 'Переключись в игру, встань ровно и не трогай мышь — через секунду камера повернётся на 360°.'
        : 'Поворачиваю…'}</div>`
      return
    }
    if (step === 'ask') {
      el.innerHTML = `${value}<div class="turn-box">
        <div class="turn-q">Насколько повернулась камера?</div>
        <button class="turn-exact" data-angle="360">Ровно на круг</button>
        <div class="turn-grid">${ANGLES.map(([l, a]) => `<button data-angle="${a}" title="${a}°">${l} круга</button>`).join('')}</div>
        <label class="turn-custom">или примерно <input id="turn-deg" type="number" min="10" max="3600" step="5" placeholder="°" /> °</label>
        <button class="link" data-act="cancel">Отмена</button>
      </div>`
      return
    }
    const live = isActive()
    el.innerHTML = `${value}
      ${message ? `<div class="turn-msg">${message}</div>` : ''}
      <button class="ghost turn-start" data-act="start" ${live ? '' : 'disabled'}>Проверить поворот на 360°</button>
      ${live ? '' : '<p class="side-hint">Доступно, когда эта игра запущена.</p>'}`
  }

  const answer = async (angle: number) => {
    const b = getViewed()
    if (!b || !(angle > 0)) return
    if (angle === 360) {
      message = 'Калибровка верная — оставил как есть.'
    } else {
      const old = b.real_world
      const next = (old * 360) / angle
      try {
        await invoke('set_real_world', { file: b.file, value: next })
        message = `real_world ${+old.toFixed(4)} → ${+next.toFixed(4)}. Проверь ещё раз.`
        onSaved()
      } catch (err) {
        message = String(err)
      }
    }
    step = 'done'
    render()
  }

  el.addEventListener('click', (ev) => {
    const t = (ev.target as HTMLElement).closest<HTMLElement>('button')
    if (!t) return
    if (t.dataset.act === 'start') {
      message = ''
      step = 'armed'
      void invoke('test_turn')
      render()
    } else if (t.dataset.act === 'cancel') {
      step = 'idle'
      render()
    } else if (t.dataset.angle) {
      void answer(Number(t.dataset.angle))
    }
  })
  el.addEventListener('change', (ev) => {
    const input = ev.target as HTMLInputElement
    if (input.id === 'turn-deg' && input.value) void answer(Number(input.value))
  })

  return {
    render,
    draw(s: PadState) {
      const turning = s.engine.turning
      if (turning && step === 'armed') {
        step = 'turning'
        render()
      } else if (!turning && wasTurning && step === 'turning') {
        step = 'ask'
        render()
      }
      wasTurning = turning
    },
  }
}
