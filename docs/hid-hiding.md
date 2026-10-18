# Скрытие DualSense от игры и виртуальный геймпад

Исследование для aikeymapper: как маппер на Rust сам прячет физический DualSense от игры,
при необходимости подставляет виртуальный геймпад и гарантированно возвращает всё как было,
даже если упал. Дата: 2026-10-18. Кода приложения здесь нет, только факты, решения и
набросок модуля.

## Коротко: рекомендация

1. **Скрытие — через HidHide, но без HidHideCLI.** Маппер напрямую шлёт IOCTL в
   `\\.\HidHide`. Права администратора для этого не нужны: проверено на этой машине,
   CLI работает из неэлевированной оболочки. Админ и перезагрузка нужны только один раз,
   при установке драйвера.
2. **Защита от падения: два режима, выбор по возможностям драйвера.**
   - Драйвер 1.7+ поддерживает *session blacklist* (IOCTL 2056/2057). Записи в нём живут,
     пока жив процесс, который их добавил, и ядро само снимает их, когда процесс
     завершается, в том числе при падении или kill. Это идеальный механизм, но подписанной
     сборки 1.7 сейчас нет (см. §1.1). Маппер должен уметь им пользоваться и включать его,
     как только драйвер его поддерживает.
   - Драйвер 1.5.230 (последний подписанный, стоит у пользователя) знает только
     постоянный список в реестре. Значит, нужны журнал изменений, процесс-сторож и
     восстановление при следующем запуске или входе в систему (§4.4).
3. **Маппер трогает только свои записи.** У пользователя уже есть своя конфигурация
   HidHide. README HidHide прямо просит не считать конфигурацию своей. Поэтому маппер
   удаляет только то, что сам добавил и записал в журнал.
4. **Виртуальный геймпад для v1 не нужен.** В режиме «клава-мышь» физический пад скрыт и
   виртуальный не нужен. В «гибриде» физический пад и так виден игре. Виртуальный нужен
   только будущему режиму «пад → пад» (например, гироскоп в стик для игры без поддержки
   мыши). Под него заложить трейт `VirtualPad` с двумя бэкендами:
   - **ViGEmBus**: снят с поддержки, но установлен почти везде, в том числе здесь.
     Годится для быстрого прототипа.
   - **VIIPER + usbip-win2**: живой проект, подписанный драйвер, есть крейт
     `viiper-client`. Это долгосрочная цель.
5. **Steam Input.** Steam открывает DualSense при своём старте, до маппера. HidHide
   проверяет доступ только в момент открытия устройства, поэтому уже открытый хэндл Steam
   продолжит читать пад. Рекомендованная настройка: отключить в Steam поддержку
   PlayStation-контроллеров (маппер её заменяет). Маппер должен находить запущенный Steam и
   предупреждать (§3).

---

## 1. HidHide (Nefarius)

### 1.1. Статус проекта и версии

| Что | Значение |
|---|---|
| Последний подписанный релиз | **v1.5.230**, 2024-05-11 ([release](https://github.com/nefarius/HidHide/releases/tag/v1.5.230.0)). Его же раздаёт собственный апдейтер Nefarius (`vicius.api.nefarius.systems/.../HidHide/updates.json`), winget (`Nefarius.HidHide`) и Chocolatey (`hidhide`). |
| Исходники | master живой: коммиты есть в мае и июле 2026, появились тесты и `DEVELOPER.md` с описанием IOCTL API. Теги **v1.6.297** (2025-09-14), **v1.7.333** (2026-04-30) и **v1.7.346** (2026-05-02) созданы, но **GitHub-релизов и подписанных инсталляторов для них нет**. |
| Позиция мейнтейнера | Issue [#217](https://github.com/nefarius/HidHide/issues/217) (июль 2026): мейнтейнер пишет, что на проект нет времени и что он не будет production-подписывать непроверенный код. Пре-релиз возможен только test-signed, а такой драйвер работает лишь в test mode. В README: «There is currently no capacity for any major works on HidHide». |
| Платформы | Только Windows 10/11 x64 (Intel/AMD). ARM64 официально нет, есть ручная установка через `nefcon`. |
| На этой машине | Установлен **1.5.230** (2024-06-25), служба `HidHide` запущена. Состояние: cloak **off**, inverse off. В blacklist лежат 4 инстанса DualSense (`VID_054C&PID_0CE6`): два USB HID с разных портов (`HID\VID_054C&PID_0CE6&MI_03\7&30dcb22d&…` и `…\8&98fa4b9&…`) и два Bluetooth (`BTHENUM\{00001124-…}_VID&0002054C_PID&0CE6\…` и `HID\{00001124-…}_VID&0002054c_PID&0ce6\…`). В whitelist: CLI, Client и одна пустая запись `""`. Скорее всего, это программа с тома, который сейчас не смонтирован: CLI превращает такой путь в пустую строку. Убирается командой `--app-clean`. |

Вывод: HidHide 1.5.230 в поддержке не нуждается и работает. Session blacklist есть в
коде, но конечному пользователю пока недоступен. Проектировать надо под 1.5.230, а
session-режим включать по результату проверки возможностей драйвера.

### 1.2. Установка

- Инсталлятор `HidHide_1.5.230_x64.exe`: **нужен админ и перезагрузка**. Обновление идёт в
  четыре шага: удаление → перезагрузка → установка → перезагрузка.
- По сути инсталлятор ставит INF и регистрирует `HidHide` как **upper class filter** для
  классов `HIDClass`, `XnaComposite` и `XboxComposite`. Это видно в
  `C:\Program Files\Nefarius Software Solutions\HidHide\x64\install.cmd`
  (`devcon classfilter HIDClass upper -HidHide`).
- В комплекте лежит `HidHideWatchdog.exe`. Это служба HidHide (появилась в 1.4.186). Её
  задача — не дать системе остаться без загрузки после неудачной установки. Конфигурацию
  она не восстанавливает и для нашей задачи бесполезна.
- Известная проблема: Kaspersky мешает работе белого списка ([docs](https://docs.nefarius.at/projects/HidHide/)).
- Как маппер узнаёт, что HidHide установлен (всё без админа):
  - `HKLM\SOFTWARE\Nefarius Software Solutions e.U.\HidHide\Version`: этот ключ проверяет
    официальный апдейтер;
  - `HKCR\Installer\Dependencies\NSS.Drivers.HidHide.x64\Version`: ключ интеграции из README;
  - служба `HidHide` через `OpenSCManager` + `QueryServiceStatus`;
  - удачный `CreateFileW("\\\\.\\HidHide")`. Подходит и интерфейс
    `{0C320FF7-BD9B-42B6-BDAF-49FEB9C91649}`.
- Как маппер ставит HidHide, если его нет: только **официальный подписанный
  инсталлятор**. Источник — либо `winget install --id Nefarius.HidHide -e`, либо
  скачивание с GitHub release с проверкой Authenticode (`WinVerifyTrust`, подписант
  «Nefarius Software Solutions e.U.»). Запуск с элевацией (`ShellExecuteW` + `runas`),
  затем сообщение «нужна перезагрузка». Флаги тихой установки у инсталлятора не
  документированы, поэтому сначала нормальный интерактивный режим. Сайты вида
  `vigembus.com` и `ds4windows.com.co` — **не** официальные, README HidHide прямо
  предупреждает о мошенниках.

### 1.3. HidHideCLI.exe

Справка (`HidHideCLI.exe --help`, 1.5.230):

| Команда | Действие |
|---|---|
| `--cloak-on` / `--cloak-off` / `--cloak-toggle` / `--cloak-state` | включить, выключить, переключить или показать скрытие (глобальный флаг `Active`) |
| `--dev-hide "<instance path>"` / `--dev-unhide "<…>"` | добавить устройство в blacklist или убрать из него |
| `--dev-list` | показать скрытые устройства |
| `--dev-all` / `--dev-gaming` | все HID-устройства или только игровые, вывод в **JSON** |
| `--app-reg "<полный путь>"` / `--app-unreg "<…>"` | добавить приложение в whitelist или убрать (может видеть скрытые устройства) |
| `--app-list` / `--app-clean` | показать whitelist / убрать отсутствующие приложения |
| `--inv-on` / `--inv-off` / `--inv-state` | инверсия: whitelist превращается в список тех, **от кого** прятать |
| `--cancel` | выйти без сохранения изменений |

Команды можно передавать цепочкой в одном вызове. Примеры:

```bat
HidHideCLI.exe --app-reg "G:\work\aikeymapper\target\release\aikeymapper.exe" --dev-hide "HID\VID_054C&PID_0CE6&MI_03\7&30dcb22d&0&0000" --cloak-on
HidHideCLI.exe --dev-unhide "HID\VID_054C&PID_0CE6&MI_03\7&30dcb22d&0&0000" --cloak-off
```

Особенности CLI, которые мешают:

- **CLI при каждом запуске молча добавляет себя в whitelist** (`FilterDriverProxy`,
  конструктор). Это побочный эффект.
- Путь к CLI приходится искать в реестре, нужен спаун процесса и разбор текстового вывода.
- Поэтому CLI годится для ручной отладки, а маппер работает через IOCTL.

### 1.4. Прямой IOCTL-интерфейс (без CLI)

Источники: [`Shared/HidHideIoctlContract.h`](https://github.com/nefarius/HidHide/blob/master/Shared/HidHideIoctlContract.h),
[`DEVELOPER.md`](https://github.com/nefarius/HidHide/blob/master/DEVELOPER.md),
[`HidHideCLI/src/FilterDriverProxy.cpp`](https://github.com/nefarius/HidHide/blob/master/HidHideCLI/src/FilterDriverProxy.cpp),
драйвер `HidHide/src/Logic.c`, `ControlDevice.c`, `Config.c`. Managed-эталон:
[Nefarius.Drivers.HidHide](https://github.com/nefarius/Nefarius.Drivers.HidHide) (MIT,
NuGet 3.4.0 от 2026-05-23).

**Устройство:** `\\.\HidHide`. Открывать так:
`CreateFileW(GENERIC_READ, FILE_SHARE_READ|WRITE|DELETE, OPEN_EXISTING)`.

**Коды:** `CTL_CODE(32769, N, METHOD_BUFFERED, FILE_READ_DATA)` = `0x80010000 | 0x4000 | (N << 2)`.

| N | Константа | Код | Вход | Выход | В 1.5.230 |
|---|---|---|---|---|---|
| 2048 | `IOCTL_GET_WHITELIST` | `0x80016000` | — | MULTI_SZ | да |
| 2049 | `IOCTL_SET_WHITELIST` | `0x80016004` | MULTI_SZ | — | да |
| 2050 | `IOCTL_GET_BLACKLIST` | `0x80016008` | — | MULTI_SZ | да |
| 2051 | `IOCTL_SET_BLACKLIST` | `0x8001600C` | MULTI_SZ | — | да |
| 2052 | `IOCTL_GET_ACTIVE` | `0x80016010` | — | `BOOLEAN` (1 байт) | да |
| 2053 | `IOCTL_SET_ACTIVE` | `0x80016014` | `BOOLEAN` | — | да |
| 2054 | `IOCTL_GET_WLINVERSE` | `0x80016018` | — | `BOOLEAN` | да |
| 2055 | `IOCTL_SET_WLINVERSE` | `0x8001601C` | `BOOLEAN` | — | да |
| 2056 | `IOCTL_ADD_SESSION_BLACKLIST` | `0x80016020` | MULTI_SZ | — | **нет**, только master/1.7 |
| 2057 | `IOCTL_CLR_SESSION_BLACKLIST` | `0x80016024` | — | — | **нет**, только master/1.7 |

Формат и семантика:

- **MULTI_SZ.** Строки UTF-16, каждая с нулевым терминатором, в конце ещё один ноль.
  Размер буфера в байтах чётный и включает все терминаторы. GET-запросы вызываются два
  раза: первый с пустым буфером возвращает нужный размер в `lpBytesReturned`, второй
  читает данные.
- **SET заменяет список целиком.** Добавление делается как read-modify-write.
  Блокировки между клиентами нет, но хэндл эксклюзивный (следующий пункт), и это
  фактически сериализует доступ.
- **Хэндл эксклюзивный** (`WdfDeviceInitSetExclusive(TRUE)`). Пока устройство держит
  открытым кто-то другой (открытый HidHideClient, другой маппер, DS4Windows), второй
  `CreateFile` вернёт `ERROR_ACCESS_DENIED`. Поэтому хэндл открываем на короткую
  транзакцию и сразу закрываем, а при `ACCESS_DENIED` повторяем с backoff. Держать
  хэндл открытым нельзя: это заблокирует GUI HidHide у пользователя.
- **Права.** Control device создаётся с `SDDL_DEVOBJ_SYS_ALL_ADM_RWX_WORLD_RWX_RES_RWX`,
  то есть **любой пользователь** может читать и менять конфигурацию. Это подтверждено:
  `HidHideCLI --cloak-state --dev-list --app-list` сработал из оболочки без элевации (в
  той же оболочке `reg query HKLM\…\HidHide\Parameters` вернул Access denied).
- **Whitelist** хранит пути в NT-форме: `\Device\HarddiskVolume3\…\aikeymapper.exe`, а не
  `G:\…`. Конвертация делается так же, как в `Volume.cpp`: `GetVolumePathNameW` →
  `GetVolumeNameForVolumeMountPointW` → `QueryDosDeviceW`. Драйвер сопоставляет путь
  образа процесса, который запомнил при создании процесса (`PsSetLoadImageNotifyRoutine`).
  При каждом SET_WHITELIST кэш вердиктов сбрасывается.
- **Blacklist** хранит device instance path в формате `SetupDiGetDeviceInstanceId`
  (`HID\VID_054C&PID_0CE6&MI_03\7&…`). Суффикс `!N` прячет устройство во всех WTS-сессиях,
  кроме сессии N (session jail, для нас не нужен).
- **Где хранится.** Постоянная конфигурация лежит в реестре:
  `HKLM\SYSTEM\CurrentControlSet\Services\HidHide\Parameters\{WhitelistedFullImageNames,
  BlacklistedDeviceInstancePaths, Active, WhitelistedInverse}`. Она **переживает и падение
  маппера, и перезагрузку**, и в этом корень риска «пад пропал навсегда».
- **Session blacklist (2056/2057, только 1.7+).** Записи живут только в памяти ядра, в
  реестр не пишутся и принадлежат процессу, который их добавил. Когда этот процесс
  завершается по любой причине, драйвер снимает его записи в `PsSetCreateProcessNotifyRoutine`
  (`SessionBlacklistCleanupForPid`). Закрытие хэндла записи **не** снимает, так что
  эксклюзивный хэндл можно отпускать. `CLR` снимает записи только вызывающего процесса.
  Учтите: session-записи **работают только при `Active = true`** (проверка `GetActive()`
  идёт раньше `Blacklisted()`), и владелец сам **тоже должен быть в whitelist**, иначе
  перестанет видеть пад.
- **Как узнать, есть ли session API.** Послать `IOCTL_CLR_SESSION_BLACKLIST` без буферов.
  В 1.5.230 сработает ветка `default` и вернётся `ERROR_INVALID_PARAMETER`. В 1.7+ вызов
  пройдёт успешно, и побочных эффектов у него нет.

Когда драйвер решает, пустить ли процесс к устройству (`OnDeviceFileCreate`):

```
если pid != 4 (System), Active и устройство в blacklist (постоянном или session):
    пускать, только если процесс в whitelist (с учётом inverse)
```

Следствия для маппера:

1. **Проверка идёт только в момент `CreateFile`.** Процесс, который уже открыл устройство
   (Steam, запущенная игра), читает его дальше. Скрытие действует на тех, кто откроет
   устройство после. Обратное тоже верно: если раскрыть пад, игра, которая уже
   перечислила устройства, его не заметит. HidHide не шлёт уведомление о подключении, так
   что игре нужен рестарт или переподключение пада.
   Принудительный способ — перезапустить devnode (`CM_Disable_DevNode` + `CM_Enable_DevNode`
   или `pnputil /restart-device`). Тогда все хэндлы закрываются и устройство открывают
   заново. Это требует **админа**, поэтому это работа для привилегированного помощника
   (§4.5) или подсказка «переподключите геймпад».
2. **Instance path зависит от порта и транспорта.** USB-порт и Bluetooth дают разные
   пути, это видно по конфигурации на этой машине. Маппер прячет **каждый** инстанс
   DualSense, который видит. Чтобы найти все, перечисляем `GUID_DEVINTERFACE_HID` с
   `VID_054C&PID_0CE6` (DualSense), `0DF2` (Edge) и `05C4`/`09CC` (DS4), группируем по
   `DEVPKEY_Device_ContainerId` и добавляем все HIDClass-узлы контейнера: так делает и
   HidHideClient. Подписываемся на появление устройств (`CM_Register_Notification`) и
   досылаем новые пути. Между подключением пада и нашим IOCTL есть короткое окно, в
   которое игра успеет открыть пад. Для 1.5.230 его можно закрыть, если заранее держать
   известные пути в blacklist (§4.4, стратегия «журнал»).
3. **Прятать HID-инстанс**, а не USB-родителя. USB-родитель DualSense HidHide не
   фильтрует, и на нём висит ещё звук. Скрывать USB-инстанс имеет смысл только для
   XInput-устройств ([#39](https://github.com/nefarius/HidHide/issues/39)), а DualSense к
   ним не относится.
4. **Inverse-режим** («прятать только от `game.exe`») по смыслу подошёл бы под профиль
   игры. Но это глобальный флаг, и он ломает конфигурацию пользователя (DS4Windows и
   ручные правила). По умолчанию его не трогаем.
5. **Сторонние API.** Демка `demo/` (gilrs 0.11) на Windows читает через
   Windows.Gaming.Input. В issue [#216](https://github.com/nefarius/HidHide/issues/216)
   есть отдельные игры (Forza Motorsport), от которых скрыть не удаётся. Поэтому кроме
   TestRange стоит проверить хотя бы одну игру на SDL/XInput и одну на GameInput.

---

## 2. Виртуальный контроллер

### 2.1. ViGEmBus — снят с поддержки (проверено)

- Репозиторий **архивирован 2023-11-02**, последний релиз **v1.22.0** вышел в тот же день
  ([GitHub](https://github.com/nefarius/ViGEmBus)).
- Причины ([EOL statement](https://docs.nefarius.at/projects/ViGEm/End-of-Life/)):
  конфликт товарного знака с ViGEM GmbH (обнаружен в мае 2023, домен `vigem.org` передан
  в декабре 2023), а также нехватка времени у единственного мейнтейнера. Обновлений не
  будет, на существующие установки это не влияет. Преемника официально не названо.
- На этой машине стоит **1.17.333** (установлен 2025-05-06), служба `ViGEmBus` запущена.
  Это старее последнего 1.22.0.
- Драйвер подписан и работает. Риски: исправлений не будет, и неизвестно, как поведут
  себя будущие сборки Windows и HVCI. Последнее — предположение, а не факт.
- Rust-крейты:
  - [`vigem-client`](https://crates.io/crates/vigem-client) (CasualX): 0.1.4, 2022,
    чистый Rust, около 21 тыс. недавних загрузок. Стабилен, но заброшен.
  - [`vigem-rust`](https://crates.io/crates/vigem-rust): 0.2.0, август 2026, Xbox 360 и
    DS4. Живой, но маленький (8 звёзд).
- Эмулирует Xbox 360 и DualShock 4. DualSense не умеет.

### 2.2. VIIPER + usbip-win2 — живая альтернатива

- [VIIPER](https://github.com/Alia5/VIIPER): виртуальные USB-устройства через USB/IP,
  вся логика устройств в user mode. Последний релиз **v0.9.0** (2026-10-10), dev-snapshot
  от 2026-10-15.
- Эмулирует **Xbox 360, DualShock 4, DualSense (и Edge, с IMU и тачпадом), Switch 2 Pro,
  HID-клавиатуру и HID-мышь**.
- На Windows нужен драйвер [usbip-win2](https://github.com/vadimgrn/usbip-win2): подписан
  (attestation/WHLK через OSSign), последний релиз v0.9.8.1 (2026-09-27), Win10 1903+ x64
  и Win11 ARM64. Установка требует админа и **перезапускает все USB 3.0 хабы**, автор
  советует перед ней создать точку восстановления.
- Rust: [`viiper-client`](https://crates.io/crates/viiper-client) 0.9.0 (2026-10-10).
  Клиентские библиотеки под MIT.
- **Лицензия.** Сам VIIPER распространяется под GPL-3.0. Если ходить к отдельному
  `viiper.exe` по TCP API, приложение может не быть GPL. Встраивание `libVIIPER`
  потянет GPL на наш код. Значит, схема: отдельный процесс-сервер плюс `viiper-client`.
- Сигнал зрелости: форк DS4Windows 5 (hbashton, пока пре-релиз) полностью перешёл с
  ViGEmBus на VIIPER.
- Неизвестно: задержка цепочки localhost TCP → USB/IP → драйвер. Её надо измерить до
  того, как делать VIIPER основным бэкендом.
- Бонус на будущее: виртуальные **HID**-клавиатура и мышь. Это путь для игр, которые
  игнорируют `SendInput`. Пока не проверено.

### 2.3. Прочее (не подходит или нишевое)

- **vJoy** даёт только DirectInput-джойстик. Шутеры на XInput и GameInput его не видят,
  для нас не подходит.
- **`Windows.UI.Input.Preview.Injection.InputInjector`**
  (`InitializeGamepadInjection`/`InjectGamepadInput`). Это штатный WinRT, драйвера не
  нужно. Но нужна restricted capability `inputInjectionBrokered`, а значит упакованное
  (MSIX) приложение. Видно ли такой пад через XInput, по документации не ясно, надо
  проверять. Рассматривать только как эксперимент.
- **Свой драйвер** (KMDF/VHF): нужны EV-сертификат и attestation-подпись Microsoft.
  Дорого, смысла нет, пока жив VIIPER.

### 2.4. Решение по виртуальному паду

- Трейт `VirtualPad { connect, update(state), feedback() -> rumble/led, disconnect }` и
  бэкенды `ViGEm` и `Viiper`. Выбор при запуске: что установлено, тем и пользуемся, при
  наличии обоих — по настройке.
- Порядок работ: сначала ViGEm (через `vigem-client` или прямой IOCTL: драйвер уже стоит,
  прототип выйдет за день). Затем VIIPER как бэкенд по умолчанию для новых установок.
- **Маппер не должен захватить свой же виртуальный пад.** У виртуального DS4/DualSense
  те же VID/PID, что у настоящего. Отличать по instance path или родителю: корень
  `ViGEmBus` или виртуальный хаб usbip VHCI. И никогда не класть такие пути в blacklist.

---

## 3. Конфликт со Steam Input

Что делает Steam (Valve [docs](https://partner.steamgames.com/doc/features/steam_controller/getting_started_for_devs),
обсуждения SDL и форумов):

- Если в Steam включена поддержка PlayStation-контроллеров («Steam Input for PlayStation
  controllers», в старом UI — «PlayStation Configuration Support»), Steam сам читает
  DualSense. Игре, запущенной из Steam, он отдаёт **виртуальный Xbox-пад**, который
  намеренно не сообщает настоящий VID/PID.
- Физический пад Steam скрывает от игры не полностью. Игры и библиотеки, которые читают
  HID напрямую, видят оба устройства, и получается двойной ввод.
- Вне игр Steam применяет desktop/Big Picture-раскладку: стики и тачпад работают как мышь
  и клавиши. Вместе с нашим маппером это двойной ввод уже на рабочем столе.
- Steam пишет в пад output-репорты (подсветка, светодиоды игрока, режим отчётов
  Bluetooth). Маппер тоже будет управлять rumble, подсветкой и адаптивными курками, и
  они начнут перетягивать устройство друг у друга.
- Наш **виртуальный** Xbox-пад Steam тоже увидит. При включённой «Xbox Configuration
  Support» Steam наложит на него свою раскладку поверх нашей.

Как разводим:

| Режим профиля | Что делает маппер | Что нужно от Steam |
|---|---|---|
| Клава-мышь (пад скрыт) | `steam.exe` **не** добавляется в whitelist. Если Steam открыл пад раньше включения скрытия, его хэндл остаётся. Тогда нужен рестарт devnode через помощника или переподключение пада. | Рекомендуется отключить поддержку PlayStation-контроллеров глобально. Маппер её заменяет. |
| Гибрид (пад виден игре) | Ничего не прячет. | Отключить Steam Input для этой игры: «Свойства → Контроллер → Отключить Steam Input». Иначе игра получит и пад Steam, и настоящий. |
| Пад → пад (виртуальный) | Физический пад скрыт, виртуальный создан. | Отключить «Xbox Configuration Support» или Steam Input для этой игры, чтобы Steam не перемаппил виртуальный пад. |

Что маппер делает автоматически: находит процесс `steam.exe`. Найти, кто держит хэндл
HID-устройства, без драйвера нельзя, поэтому эвристика простая: Steam запущен и Steam
Input для PS включён → в UI предупреждение со ссылкой на настройку. Читать
`config/localconfig.vdf` и `config.vdf` Steam ради точного флага можно, но это
недокументированный формат. Отложить.

---

## 4. Набросок модуля на Rust

### 4.1. Состав

```
crates/
  hidhide/          # тонкая обёртка над IOCTL; зависит только от `windows`
    src/lib.rs      # HidHide, Error, Capabilities
    src/ioctl.rs    # коды, MULTI_SZ
    src/ntpath.rs   # DOS-путь → \Device\HarddiskVolumeN\…
  devices/          # перечисление DualSense/DS4, ContainerId, hotplug
  cloak/            # политика: режимы профиля, журнал, сторож, восстановление
```

Фичи крейта `windows`: `Win32_Foundation`, `Win32_Storage_FileSystem`, `Win32_System_IO`,
`Win32_System_Ioctl`, `Win32_Devices_DeviceAndDriverInstallation`, `Win32_Devices_HumanInterfaceDevice`,
`Win32_System_Threading`, `Win32_System_Services`, `Win32_System_Registry`, `Win32_Security_WinTrust`.

### 4.2. `hidhide`: доступ к драйверу

```rust
pub const DEVICE: &str = r"\\.\HidHide";
const fn ctl(n: u32) -> u32 { 0x8001_0000 | 0x4000 | (n << 2) } // CTL_CODE(32769, n, BUFFERED, FILE_READ_DATA)
pub const GET_WHITELIST: u32 = ctl(2048);
pub const SET_WHITELIST: u32 = ctl(2049);
pub const GET_BLACKLIST: u32 = ctl(2050);
pub const SET_BLACKLIST: u32 = ctl(2051);
pub const GET_ACTIVE: u32 = ctl(2052);
pub const SET_ACTIVE: u32 = ctl(2053);
pub const ADD_SESSION_BLACKLIST: u32 = ctl(2056);
pub const CLR_SESSION_BLACKLIST: u32 = ctl(2057);

pub enum Status {
    NotInstalled,                       // нет службы и устройства
    Installed { version: Option<String>, session_api: bool },
    Busy,                               // ERROR_ACCESS_DENIED: хэндл держит HidHideClient и т.п.
}

pub struct HidHide; // без постоянного хэндла: устройство эксклюзивное

impl HidHide {
    pub fn probe() -> Status;

    /// Открыть \\.\HidHide на одну транзакцию; ACCESS_DENIED → повтор с backoff (до ~2 с).
    fn tx<T>(f: impl FnOnce(&Handle) -> Result<T>) -> Result<T>;

    pub fn active(&self) -> Result<bool>;
    pub fn set_active(&self, on: bool) -> Result<()>;
    pub fn blacklist(&self) -> Result<Vec<String>>;
    pub fn set_blacklist(&self, paths: &[String]) -> Result<()>;
    pub fn whitelist(&self) -> Result<Vec<String>>;       // NT-пути
    pub fn set_whitelist(&self, images: &[String]) -> Result<()>;

    /// Атомарный read-modify-write в одной транзакции.
    pub fn edit(&self, f: impl FnOnce(&mut Config)) -> Result<Config /* до правки */>;

    /// Только 1.7+: записи живут, пока жив текущий процесс.
    pub fn session_hide(&self, paths: &[String]) -> Result<()>;
    pub fn session_clear(&self) -> Result<()>;
}

pub fn encode_multi_sz(items: &[String]) -> Vec<u16>;   // "a\0b\0\0"; пустой список → "\0\0"
pub fn decode_multi_sz(buf: &[u16]) -> Vec<String>;
pub fn dos_to_nt_path(p: &Path) -> Result<String>;      // как HidHide::FileNameToFullImageName
```

### 4.3. Режимы профиля

```rust
pub enum PadVisibility {
    Hidden,         // «только клава-мышь» и «пад → пад»: игра не видит физический DualSense
    Visible,        // «гибрид»: игра видит пад, маппер читает параллельно
}

pub trait Cloak {
    fn ensure_self_whitelisted(&mut self) -> Result<()>; // путь своего exe; постоянная запись, безвредна
    fn apply(&mut self, v: PadVisibility, pads: &[PadInstance]) -> Result<ApplyOutcome>;
    fn restore(&mut self) -> Result<()>;                  // вернуть всё, что сделали мы
}

pub enum ApplyOutcome {
    Done,
    /// Кто-то открыл пад раньше скрытия: Steam или уже запущенная игра.
    NeedsReopen { via_helper: bool },
}
```

Когда применять: решение принимается **до того, как игра откроет устройство**.

- Профиль привязан к exe игры. Маппер опрашивает процессы (`EnumProcesses` раз в
  ~250 мс, админ не нужен) или сам запускает игру из своего UI, и это лучший вариант.
- Смена профиля по фокусу окна посреди игры сработает только с рестартом devnode или
  переподключением. В UI это должно быть явно сказано.
- В режиме `Hidden` маппер выставляет `Active = true`, а это глобальный флаг. Записи
  пользователя в blacklist при этом тоже начинают действовать. Это ожидаемо, но в журнал
  записывается исходное значение `Active`.

### 4.4. Две стратегии надёжности

**A. `SessionCloak` (драйвер 1.7+, `session_api == true`).**

```
start:  ensure_self_whitelisted; journal{prev_active}; set_active(true); session_hide(pads)
hotplug: session_hide(new_pad)
Visible: session_clear()            (prev_active вернуть, если мы его меняли)
exit/crash: ядро само снимает session-записи
```

Остаётся только флаг `Active`. Если маппер упал, пока `Active = true`, а у пользователя
постоянный blacklist пуст, это безвредно. Если не пуст, возвращаем флаг по журналу при
следующем старте (как в B).

**B. `JournaledCloak` (драйвер 1.5.230, текущий).**

Журнал лежит в `%LOCALAPPDATA%\aikeymapper\hidhide-journal.json`. Пишется **до** каждой
мутации, атомарно (temp + `MoveFileExW(REPLACE_EXISTING|WRITE_THROUGH)`):

```json
{ "owner_pid": 1234, "owner_start": 133...,  "dirty": true,
  "prev_active": false,
  "added_blacklist": ["HID\\VID_054C&PID_0CE6&MI_03\\7&30dcb22d&0&0000"],
  "added_whitelist": [] }
```

- `restore()`:
  - снимает только `added_*`: если пользователь сам добавил то же значение позже, мы
    его не трогаем, потому что `added_*` пишется только для путей, которых в списке не
    было;
  - возвращает `Active` в `prev_active`;
  - ставит `dirty = false`.
- Кто вызывает `restore()`, по уровням:
  1. **Чистый выход**: `Drop` у `CloakGuard`, `SetConsoleCtrlHandler`, `WM_QUERYENDSESSION`
     и `WM_ENDSESSION`, `std::panic::set_hook` (до abort).
  2. **Сторож**: отдельный крошечный процесс `aikeymapper-guard.exe`, который маппер
     запускает при старте.
     - Сторож делает `OpenProcess(SYNCHRONIZE)` на маппер и ждёт
       `WaitForSingleObject`. Когда маппер завершился, сторож проверяет `dirty` и, если
       нужно, вызывает `restore()`.
     - Сторож не должен состоять в Job Object маппера с `KILL_ON_JOB_CLOSE`, иначе он
       умрёт вместе с маппером. Права не нужны: IOCTL доступен всем.
     - Сторож переживает падение и kill маппера. Он **не** переживает «Завершить дерево
       процессов», BSOD и выключение питания.
  3. **Следующий запуск маппера**: первым делом журнал. Если `dirty`, а `owner_pid` мёртв
     (или PID уже занят процессом с другим `owner_start`), то `restore()`.
  4. **Вход в систему**: `HKCU\…\Run` → `aikeymapper.exe --restore-only`. Команда
     восстанавливает конфигурацию и сразу выходит. Это нужно на случай, когда после BSOD
     маппер не запускают вручную.
  5. **Ручной аварийный выход**: в README и в трее «Пад пропал? → `aikeymapper --restore`»,
     плюс запасной вариант `HidHideCLI.exe --cloak-off`.
- Окно гонки при hotplug для 1.5.230: известные пути DualSense (журнал прошлых сессий)
  можно держать в постоянном blacklist заранее, а включать и выключать только `Active`.
  Минус: `Active` глобальный. Использовать как опцию «агрессивное скрытие».

### 4.5. Привилегированный помощник (опционально, после v1)

Маленькая служба `aikeymapper-svc` (LocalSystem). Это **не** pid 4, так что HidHide её не
пропускает, но пад ей читать и не нужно. Её задачи:

- рестарт devnode DualSense после включения скрытия (`ApplyOutcome::NeedsReopen`). Так
  решается проблема «Steam и уже запущенная игра держат хэндл»;
- роль сторожа, который переживает и «Завершить дерево процессов»;
- установка HidHide, ViGEm и usbip-win2 с одним запросом UAC на всё.

Без службы маппер целиком работает от обычного пользователя. Админ нужен только для
однократной установки драйверов.

### 4.6. Порядок старта маппера

```
1. journal: dirty && owner мёртв → restore()
2. probe HidHide:
     NotInstalled → режим «без скрытия» + кнопка «Установить HidHide» (UAC, перезагрузка)
     Busy         → повтор; потом подсказка «закройте HidHide Configuration Client»
     Installed{session_api} → SessionCloak | JournaledCloak
3. ensure_self_whitelisted()           — проверить в TestRange: маппер видит пад, игра нет
4. spawn guard (если JournaledCloak)
5. devices: перечислить DualSense, подписаться на hotplug
6. по профилю активной игры: cloak.apply(Hidden|Visible)
7. steam.exe запущен → предупреждение §3
```

### 4.7. Проверка на TestRange

- Скрытие работает, если панель «Геймпадов видит игра» показывает 0, а маппер при этом
  получает ввод.
- Падение: `taskkill /F /IM aikeymapper.exe`. Через ~1 с, после сторожа, перезапущенный
  TestRange снова видит пад.
- Отдельный тест: TestRange запущен **до** включения скрытия. Ожидаем, что он видит пад
  дальше (§1.4, п. 1), пока не будет рестарта devnode.

---

## Источники

- HidHide: [репозиторий](https://github.com/nefarius/HidHide) ·
  [релизы](https://github.com/nefarius/HidHide/releases) ·
  [DEVELOPER.md (IOCTL API)](https://github.com/nefarius/HidHide/blob/master/DEVELOPER.md) ·
  [HidHideIoctlContract.h](https://github.com/nefarius/HidHide/blob/master/Shared/HidHideIoctlContract.h) ·
  [FilterDriverProxy.cpp](https://github.com/nefarius/HidHide/blob/master/HidHideCLI/src/FilterDriverProxy.cpp) ·
  [issue #217 (релиз отстаёт от исходников)](https://github.com/nefarius/HidHide/issues/217) ·
  [issue #39 (USB vs HID инстансы)](https://github.com/nefarius/HidHide/issues/39) ·
  [issue #216 (Forza)](https://github.com/nefarius/HidHide/issues/216) ·
  [docs.nefarius.at/projects/HidHide](https://docs.nefarius.at/projects/HidHide/) ·
  [Nefarius.Drivers.HidHide (managed API)](https://github.com/nefarius/Nefarius.Drivers.HidHide) ·
  [winget Nefarius.HidHide](https://winstall.app/apps/Nefarius.HidHide)
- ViGEm: [ViGEmBus (archived)](https://github.com/nefarius/ViGEmBus) ·
  [End-of-Life statement](https://docs.nefarius.at/projects/ViGEm/End-of-Life/) ·
  [vigem-client](https://crates.io/crates/vigem-client) · [vigem-rust](https://crates.io/crates/vigem-rust)
- VIIPER: [репозиторий](https://github.com/Alia5/VIIPER) ·
  [DualSense](https://github.com/Alia5/VIIPER/blob/main/docs/devices/dualsense.md) ·
  [viiper-client](https://crates.io/crates/viiper-client) ·
  [usbip-win2](https://github.com/vadimgrn/usbip-win2) ·
  [OpenController issue про VIIPER как замену ViGEmBus](https://github.com/Brunovncs/OpenController/issues/1)
- DS4Windows: [companion drivers](https://kanuan.github.io/DS4WSite/about/companion-drivers/)
- Steam Input: [Steamworks: Getting started for devs](https://partner.steamgames.com/doc/features/steam_controller/getting_started_for_devs) ·
  [SDL: real VID/PID за виртуальным контроллером Steam](https://discourse.libsdl.org/t/sdl-use-the-real-device-vid-pid-when-seeing-the-steam-virtual-controller/34810) ·
  [обзор двойного ввода Steam Input](https://bugnet.io/blog/fix-steam-input-controller-double-input)
- InputInjector: [InjectGamepadInput](https://learn.microsoft.com/en-us/uwp/api/windows.ui.input.preview.injection.inputinjector.injectgamepadinput?view=winrt-26100)
- Локальные проверки (2026-10-18, без админа): `HidHideCLI.exe --help`,
  `--version --cloak-state --inv-state --dev-list --app-list`, `install.cmd`/`uninstall.cmd`,
  `sc query HidHide|ViGEmBus`, ключи Uninstall в реестре.
