# Dungeonborne community framework

Набор, чтобы поднять **свой сервер сообщества** для Dungeonborne — онлайн-игры, чьи официальные серверы закрыты:
сервер (эмулятор), лаунчер для игроков, редактор баланса, моды UE4SS и инструменты разработчика.

A kit to run **your own community server** for Dungeonborne, an online game whose official servers were shut down:
the server emulator, a players' launcher, a balance editor, UE4SS mods and developer tools.

## Как начать / How to start

Дайте ссылку на этот репозиторий своему ИИ-помощнику, который умеет запускать команды (Claude Code, Codex и т. п.),
и попросите «проверь и установи». Он прочитает `AGENTS.md`: проверит скрипт `bootstrap.ps1`, покажет план, после
вашего «да» поставит всё в одну папку по шагам и сам разберётся со сбоями. Права администратора не нужны. Около
5 минут, ~0,3 ГБ загрузки, ~3,5 ГБ на диске.

Give this repository's link to an AI assistant that can run commands and ask it to "check and set it up". It follows
`AGENTS.md`: audits `bootstrap.ps1`, shows you the plan, installs into one folder step by step after your "yes" and
handles failures. No administrator rights. About 5 minutes, ~0.3 GB download, ~3.5 GB on disk.

Сами / by hand (Windows 10/11, PowerShell). Скрипт клонирует `https://github.com/dungeonborne-community/dungeonborne`;
для своего форка добавьте `-Repo <url>` / it clones this repository; add `-Repo <url>` for your fork:

```powershell
powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root C:\DBDev          # только план / plan only
powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root C:\DBDev -Apply   # установка / install
powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root C:\DBDev -Status  # что сделано / what is done
```

После сбоя запустите ту же команду ещё раз: готовые шаги пропускаются / after a failure run it again: finished steps
are skipped.

Нужно заранее / you need: Windows 10/11, интернет, ~3,5 ГБ (без Visual Studio Build Tools ~5,5 ГБ; для сборки
сервера и лаунчера потом нужно больше — скрипт предупреждает, если свободно меньше 20 ГБ). Для настоящего сервера — своя копия игры,
машина под сервер и свои ключи (`AGENTS.md`, «What the person needs»).

## Что внутри / What is inside

| | |
|---|---|
| `core/backend` | сервер (Rust) / game server emulator |
| `apps/launcher` | лаунчер игрока / players' launcher |
| `apps/studio` | Balance Studio — правка баланса, результат — патч / balance editor, produces a patch |
| `apps/devkit`, `tools/ws`, `xtask` | установка и проверки рабочего места / workspace setup and checks |
| `game/ue4ss`, `game/client-mod` | моды клиента и сервера боя / client and dedicated-server mods |
| `apps/discord-bot` | Discord-бот сообщества / community Discord bot |
| `content` | сезоны, таблицы баланса и лута / seasons, balance and loot tables |
| `ops` | выкладка, сервер боя, резервные копии, Forgejo / deploy, battle-server box, backups, Forgejo |
| `research`, `docs` | исследования протокола и клиента, документы / protocol and client research, documents |

С чего читать / where to start: история — `docs/HISTORY.md`; как устроен прод — `docs/CLOUD_ARCHITECTURE.md`; что
лежит вне git и как это получить — `docs/STORAGE_EXPORT.md`; словарь — `docs/GLOSSARY.md`.

Таблицы и данные игры, нужные серверу, здесь есть; кода издателя и двоичных файлов (паков, дампов) нет — они
собираются из вашей копии игры (`docs/DISCLAIMER.md`, `docs/STORAGE_EXPORT.md`). / Tables and game data the server
needs are here; the publisher's code and binaries (paks, dumps) are not — they are rebuilt from your own copy of the
game. Ошибки безопасности — `.github/SECURITY.md`. Лицензия — MIT или Apache-2.0 на выбор (`LICENSE-MIT`,
`LICENSE-APACHE`); чужие программы — `docs/THIRD_PARTY.md`.
