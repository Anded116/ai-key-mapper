/** Клавиши движка: как их показать и как поймать с клавиатуры. */

/** ЙЦУКЕН поверх QWERTY: что игра покажет на русской раскладке («нажми Ы»). */
const RU: Record<string, string> = {
  Q: 'Й', W: 'Ц', E: 'У', R: 'К', T: 'Е', Y: 'Н', U: 'Г', I: 'Ш', O: 'Щ', P: 'З',
  A: 'Ф', S: 'Ы', D: 'В', F: 'А', G: 'П', H: 'Р', J: 'О', K: 'Л', L: 'Д',
  Z: 'Я', X: 'Ч', C: 'С', V: 'М', B: 'И', N: 'Т', M: 'Ь',
  LeftBracket: 'Х', RightBracket: 'Ъ', Semicolon: 'Ж', Quote: 'Э', Comma: 'Б', Period: 'Ю', Backquote: 'Ё',
}

const SYMBOL: Record<string, string> = {
  LeftBracket: '[', RightBracket: ']', Semicolon: ';', Quote: "'", Comma: ',', Period: '.', Backquote: '`',
  Minus: '−', Equals: '=', Slash: '/', Backslash: '\\',
}

const WORD: Record<string, string> = {
  Space: 'Пробел', LShift: 'Shift', RShift: 'Shift', LCtrl: 'Ctrl', RCtrl: 'Ctrl', LAlt: 'Alt', RAlt: 'Alt',
  Esc: 'Esc', Tab: 'Tab', Enter: 'Enter', Backspace: '⌫', CapsLock: 'Caps',
  MouseLeft: 'ЛКМ', MouseRight: 'ПКМ', MouseMiddle: 'СКМ', MouseBack: 'М4', MouseForward: 'М5',
  WheelUp: 'Колесо ↑', WheelDown: 'Колесо ↓', Up: '↑', Down: '↓', Left: '←', Right: '→',
  Home: 'Home', End: 'End', PageUp: 'PgUp', PageDown: 'PgDn', Insert: 'Ins', Delete: 'Del',
  ToggleMovement: 'WASD ↔ стик', ToggleVehicle: 'техника',
}

/** Названия на английском — для игр, которые пишут подсказки по-английски. */
const WORD_EN: Record<string, string> = {
  Space: 'Space', MouseLeft: 'LMB', MouseRight: 'RMB', MouseMiddle: 'MMB', MouseBack: 'M4', MouseForward: 'M5',
  WheelUp: 'Wheel ↑', WheelDown: 'Wheel ↓',
}

/** Подсказка к клавише в редакторе и в тултипе. */
const HINT: Record<string, string> = {
  MouseLeft: 'левая кнопка мыши', MouseRight: 'правая кнопка мыши', MouseMiddle: 'средняя кнопка мыши (колесо)',
  MouseBack: 'боковая кнопка мыши «назад»', MouseForward: 'боковая кнопка мыши «вперёд»',
  ToggleMovement: 'левый стик: гибрид ↔ WASD',
  ToggleVehicle: 'режим техники: левый стик и курки — виртуальный Xbox-геймпад',
}

export interface KeyFace {
  /** Крупно: то, что напишет игра. */
  main: string
  /** Мелко: латинская клавиша для букв на русской раскладке. */
  sub?: string
  /** Длинная подпись (Пробел, Колесо ↑) — набирается мельче. */
  wide: boolean
  title: string
}

/** Как игра пишет клавиши: `ru` — буквы русской раскладки крупно (S → Ы), `en` — латиница крупно. */
let lang: 'ru' | 'en' = 'ru'

export function setPromptLang(l: string) {
  lang = l === 'en' ? 'en' : 'ru'
}

export function keyFace(name: string): KeyFace {
  const ru = RU[name]
  if (ru) {
    const latin = SYMBOL[name] ?? name
    const [main, sub] = lang === 'en' ? [latin, ru] : [ru, latin]
    return { main, sub, wide: false, title: `${ru} / ${latin}` }
  }
  const main = (lang === 'en' ? WORD_EN[name] : undefined) ?? WORD[name] ?? SYMBOL[name] ?? name
  return { main, wide: main.length > 3, title: HINT[name] ?? main }
}

/** Клавиши мыши и колесо: их не поймать с клавиатуры, в редакторе они кнопками. */
export const MOUSE_KEYS = ['MouseLeft', 'MouseRight', 'MouseMiddle', 'MouseBack', 'MouseForward', 'WheelUp', 'WheelDown']

const CODE: Record<string, string> = {
  Space: 'Space', Tab: 'Tab', Escape: 'Esc', Enter: 'Enter', Backspace: 'Backspace', CapsLock: 'CapsLock',
  ShiftLeft: 'LShift', ShiftRight: 'RShift', ControlLeft: 'LCtrl', ControlRight: 'RCtrl', AltLeft: 'LAlt', AltRight: 'RAlt',
  Minus: 'Minus', Equal: 'Equals', BracketLeft: 'LeftBracket', BracketRight: 'RightBracket', Semicolon: 'Semicolon',
  Quote: 'Quote', Comma: 'Comma', Period: 'Period', Slash: 'Slash', Backslash: 'Backslash', Backquote: 'Backquote',
  ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
  Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', Insert: 'Insert', Delete: 'Delete',
}

/** Клавиша движка по `KeyboardEvent.code`: физическая клавиша, раскладка не важна. */
export function keyFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3)
  if (/^Digit[0-9]$/.test(code)) return code.slice(5)
  if (/^F([1-9]|1[0-2])$/.test(code)) return code
  return CODE[code] ?? null
}
