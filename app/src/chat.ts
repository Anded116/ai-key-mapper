import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { AgentEvent } from './types'

/** Миничат: сообщение уходит в `claude -p`, события его потока приходят строками ленты. */
export function mountChat() {
  const feed = document.getElementById('feed')!
  const form = document.getElementById('composer') as HTMLFormElement
  const input = document.getElementById('input') as HTMLTextAreaElement
  const send = document.getElementById('send') as HTMLButtonElement
  const reset = document.getElementById('chat-reset') as HTMLButtonElement
  let thinking: HTMLElement | null = null

  const add = (cls: string, text: string) => {
    const el = document.createElement('div')
    el.className = `msg ${cls}`
    el.textContent = text
    if (thinking) feed.insertBefore(el, thinking)
    else feed.appendChild(el)
    feed.scrollTop = feed.scrollHeight
    return el
  }

  const setBusy = (busy: boolean) => {
    send.disabled = busy
    reset.disabled = busy
    if (busy && !thinking) thinking = add('thinking', 'агент думает…')
    if (!busy && thinking) { thinking.remove(); thinking = null }
  }

  listen<AgentEvent>('agent', ({ payload: e }) => {
    if (e.kind === 'done') return setBusy(false)
    add(e.kind, e.kind === 'tool' ? `→ ${e.text}` : e.text)
  })

  form.addEventListener('submit', async (ev) => {
    ev.preventDefault()
    const text = input.value.trim()
    if (!text || send.disabled) return
    add('user', text)
    input.value = ''
    setBusy(true)
    try {
      await invoke('send_chat', { text })
    } catch (err) {
      setBusy(false)
      add('error', String(err))
    }
  })

  input.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter' && !ev.shiftKey) {
      ev.preventDefault()
      form.requestSubmit()
    }
  })

  reset.addEventListener('click', async () => {
    await invoke('reset_chat')
    add('system', 'Новый диалог: следующее сообщение начнёт новую сессию.')
  })
}
