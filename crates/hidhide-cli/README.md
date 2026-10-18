# aikm-hidhide

Прячет DualSense от игр через HidHide. Работает прямыми IOCTL к `\\.\HidHide` (крейт
`crates/hidhide`), без HidHideCLI и без прав администратора. Каждое изменение сначала
записывается в журнал, и откат снимает только записанное. Пользовательские записи HidHide
не трогаются. Это стратегия «журнал» из [docs/hid-hiding.md](../../docs/hid-hiding.md) §4.4B
для драйвера 1.5.230, у которого нет session blacklist.

```bash
cargo build --release -p hidhide-cli
```

| Команда | Что делает |
|---|---|
| `aikm-hidhide status` | версия драйвера, есть ли session API, `Active`, blacklist и whitelist (наши записи помечены `[наша]`), подключённые DualSense и состояние журнала |
| `aikm-hidhide hide [<instance>...]` | добавляет недостающие узлы DualSense (по умолчанию все подключённые HIDClass-узлы с PID `0CE6`/`0DF2`) в blacklist и включает `Active`. Скрытие остаётся до `unhide`/`restore` |
| `aikm-hidhide hide --allow <exe>` | ещё и добавляет exe в whitelist (путь переводится в `\Device\HarddiskVolumeN\…`) |
| `aikm-hidhide hide --hold` | то же, но скрытие живёт, пока жив процесс. Запускает сторожа `aikm-hidhide-guard.exe`, снимает скрытие по Ctrl+C, досылает в blacklist новые узлы при переподключении пада |
| `aikm-hidhide unhide [<instance>...]` | снимает наши записи blacklist (все или указанные) и, когда наших не осталось, возвращает `Active`. Чужие пути не трогает |
| `aikm-hidhide restore` | откатывает всё из журнала, включая whitelist |

Журнал: `%LOCALAPPDATA%\aikeymapper\hidhide-journal.json`. Лог сторожа: `guard.log` рядом с ним.

Как скрытие снимается, если процесс с `--hold` не завершился штатно:

1. Ctrl+C или закрытие консоли: срабатывает обработчик, процесс откатывает журнал сам.
2. `taskkill /F` или падение: сторож ждёт процесс по pid и времени старта, затем откатывает журнал.
3. Убиты оба процесса (или был BSOD): журнал остаётся грязным, а владелец уже мёртв. Следующий
   `hide` или `unhide` сначала откатывает его. `status` показывает такое состояние.

## Проверка на TestRange (2026-10-18)

Стенд: HidHide 1.5.230 (session API нет), DualSense по Bluetooth. До начала проверки `Active`
был выключен. В blacklist пользователя лежали 4 записи, среди них BT-узел `BTHENUM\{00001124-…}`
подключённого пада. HID-узла `HID\{00001124-…}\8&D004270&3&0000`, через который пад виден сейчас,
там не было: у пользователя записан `…&2&0000` от прошлого подключения. Поэтому `hide`
добавлял в blacklist одну нашу запись (HID-узел) и включал `Active`.

TestRange запускался заново на каждый замер: `testrange.exe --screenshot <png>`, 60 кадров, затем
выход. На кадре смотрим строку «Геймпадов видит игра».

| # | Сценарий | Ожидание | Результат |
|---|---|---|---|
| 0 | исходное состояние | 1 | 1 (`PS5 Controller [054C:0CE6]`) |
| 1 | `hide` → TestRange | 0 | 0, три запуска подряд |
| 1 | `restore` → TestRange | 1 | 1. Вывод `status` побайтно совпал с исходным |
| 2 | `hide --hold` → TestRange (дважды) | 0 | 0 и 0 |
| 2 | `taskkill /F /IM aikm-hidhide.exe`, через 1 с TestRange | 1 | 1. В `guard.log` запись «завершился; откатил…», сторож вышел, `status` совпал с исходным |
| 3 | `hide --allow testrange.exe` → TestRange | 1: процесс в whitelist видит пад | 1. Значит, NT-путь `\Device\HarddiskVolume2\work\…\testrange.exe` драйвер сопоставил верно. `restore` снял и blacklist, и whitelist |
| 4 | `hide --hold`, затем убиты и маппер, и сторож → TestRange | 0, журнал грязный | 0. `status`: «владелец pid … мёртв» |
| 4 | `unhide` (сначала откат осиротевшего журнала) → TestRange | 1 | 1 |
| 5 | `hide --hold`, затем `restore` из другой консоли | hold выходит сам | hold завершился с кодом 0, сторож записал «журнал чистый», ничего не трогал |
| 6 | `unhide <чужой путь пользователя>` | путь не трогается | «не наша запись, не трогаю», blacklist без изменений |

После всех прогонов `status` побайтно совпал с исходным: 4 записи blacklist, 3 записи whitelist,
`Active` выключен.

Не проверялось:
- Сценарий из §4.7 «TestRange запущен до скрытия и видит пад дальше». В режиме `--screenshot`
  TestRange сразу выходит, так что этот случай надо проверять вручную в окне.
- Ctrl+C в консоли `--hold`: обработчик есть, но живой Ctrl+C не посылался. Внешний `restore`
  и `taskkill` проверены.
- Ответ `Busy` при открытом HidHide Configuration Client.

Ловушка при проверке из PowerShell: release-сборка TestRange — GUI-приложение
(`windows_subsystem = "windows"`), и `& testrange.exe` его не ждёт. Кадр снимется уже после
следующих команд. Запускать нужно через `Start-Process … -Wait` или из bash.
