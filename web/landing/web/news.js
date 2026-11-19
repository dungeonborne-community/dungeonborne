// Built by build_news.py from seasons/playtest/patch_notes.toml — do not edit by hand.
window.NEWS = [
 {
  "version": "2026.11.11",
  "date": "2026-11-11",
  "ru": {
   "title": "Обновление 11 ноября",
   "new": [
    "Сезон начат заново: база данных сервера перенесена на новое хранилище, прогресс сброшен. Новый персонаж — 1 уровня, с серой стартовой экипировкой, 10 000 золота и ресурсами реликвий по 5000.",
    "Войдите в лаунчер заново через Discord: старые входы сброшены вместе с базой.",
    "Номер боя показывается в строке пинга — укажите его, когда сообщаете об ошибке.",
    "Статистика персонажа: игры, выходы, смерти, убийства игроков и монстров, открытые сундуки, урон по игрокам и время в рейде.",
    "Аукцион: можно выставить вещь без бонуса комплекта; фильтры по классу, слоту и фиксированным свойствам работают; на главной странице появились подборки по слотам и редкости.",
    "Лобби лаунчера: для голосования достаточно 2 игроков во вкладке, зал ждёт 3 минуты."
   ],
   "fixed": [
    "Сохранения стали надёжнее: изменения персонажа больше не теряются, если сервер сохраняет их одновременно.",
    "Зелье восхождения: дополнительный опыт начисляется только если выйти из рейда живым (раньше — и при смерти). Выпитое на первом этаже больше не пропадает при переходе на второй.",
    "Камни воскрешения переходят с вами на второй этаж. Вынести их из рейда по-прежнему нельзя.",
    "Гарантированный красный портал появляется в любом месте текущего круга, а не всегда в центре.",
    "Квесты боевого пропуска на урон по игрокам снова засчитывают урон.",
    "Замок: сервер мог взять настройки другой карты, и игроки появлялись слишком близко друг к другу.",
    "Туннель боя («обход» в лаунчере) больше не включается сам — только переключателем.",
    "Анимация оружия сразу после появления в бою больше не сбивается на стандартную (раньше помогала только смена оружия). Исправлена поза с двумя кинжалами."
   ],
   "balance": [
    "Рюкзак по расе, как в оригинале: 6 столбцов, у людей 7. Вещи из лишнего столбца переносятся на свободное место или на склад.",
    "Лимит редкости карты проверяется и для рюкзака: в Замок обычной сложности нельзя взять в сумках синее и выше. Вещи не по лимиту при входе в бой уходят на склад."
   ]
  },
  "en": {
   "title": "November 11 update",
   "new": [
    "The season starts over: the server database moved to new storage and progress is reset. A new character is level 1 with grey starter gear, 10,000 gold and 5,000 of each relic resource.",
    "Log in to the launcher again through Discord: old logins were reset with the database.",
    "The battle number is shown in the ping line — include it when you report a bug.",
    "Character statistics: games, extractions, deaths, player and monster kills, opened chests, damage to players and time in raids.",
    "Auction: you can list an item without a set bonus; class, slot and fixed-property filters work; the main page now has picks by slot and rarity.",
    "Launcher lobby: 2 players in a tab are enough to start the vote; the hall waits 3 minutes."
   ],
   "fixed": [
    "Saves are more reliable: character changes are no longer lost when the server saves them at the same time.",
    "Potion of Ascension: bonus experience is granted only if you leave the raid alive (it used to count on death too). A potion drunk on the first floor no longer gets lost on the way to the second.",
    "Resurrection stones now come with you to the second floor. You still cannot take them out of the raid.",
    "The guaranteed red portal appears anywhere in the current circle, not always in the centre.",
    "Battle pass quests for damage to players count damage again.",
    "Castle: the server could take another map's settings, so players spawned too close to each other.",
    "The battle tunnel (the launcher's bypass) no longer turns itself on — only with the toggle.",
    "Weapon animations no longer fall back to the default ones right after you spawn (only swapping weapons used to help). The dual dagger idle pose is fixed."
   ],
   "balance": [
    "Backpack size depends on race, as in the original: 6 columns, 7 for humans. Items from the extra column move to a free spot or to the stash.",
    "The map's rarity limit also applies to your bags: you cannot bring blue or higher in your bags to normal Castle. Items over the limit go to the stash when you enter the battle."
   ]
  }
 },
 {
  "version": "2026.11.08",
  "date": "2026-11-08",
  "ru": {
   "title": "Обновление 8 ноября",
   "new": [
    "Сезон начат заново: прогресс сброшен. Новый персонаж — 1 уровня, с серой стартовой экипировкой и 10 000 золота. Серый комплект выдаётся и после смерти.",
    "Страховка как в оригинале: цена — от рыночной стоимости вещи и её редкости. Застраховать можно одну вещь; после смерти она остаётся у вас, а убийца получает страховую премию и может продать её торговцу.",
    "Печати лей-линий: фрагменты падают с сундуков и монстров, целые печати — с боссов, гоблина-вора и золотых сундуков. 10 фрагментов собираются в печать; печать меняется на алтаре лей-линий на сокровище.",
    "У сложности Classic и Hard теперь свой лут.",
    "Квесты боевого пропуска засчитывают убийства монстров, открытые сундуки и урон по игрокам. Пропуск продлён.",
    "Подбор как в оригинале: группа равна размеру комнаты. На Hard — режим стычки: группы любого размера в одном бою.",
    "«Режим вызова» в окне выбора карты работает: соло и дуо идут в бой против трио. В группе режим выбирает капитан.",
    "Лобби лаунчера: голосование начинается, когда во вкладке 8 игроков; зал ждёт 8 игроков (не дольше 5 минут сверх таймера). Правила подбора видны в лаунчере 1.5.1.",
    "Таблица лидеров: среднее число убийств игроков за бой в подземелье."
   ],
   "fixed": [
    "Дверь сокровищницы Виндиго открывается после его убийства.",
    "На старте второго этажа игроки больше не проваливаются под пол."
   ],
   "balance": [
    "Убрана прибавка +300 к здоровью на 20 уровне.",
    "Системные лоты на аукционе и Buyout Trades отключены — торгуют только игроки.",
    "На Hard всё снаряжение должно быть эпическим или выше, как в оригинале.",
    "Материалы кузницы при входе больше не выдаются."
   ]
  },
  "en": {
   "title": "November 8 update",
   "new": [
    "The season starts over: progress is reset. A new character is level 1 with grey starter gear and 10,000 gold. You also get the grey set after death.",
    "Insurance works like the original: the price depends on the item's market value and rarity. You can insure one item; it stays with you after death, and the killer gets an insurance premium to sell to the merchant.",
    "Leyline Seals: fragments drop from chests and monsters, whole seals from bosses, the Goblin Thief and gold chests. 10 fragments make a seal; trade a seal at a Leyline Altar for a treasure.",
    "Classic and Hard difficulties now have their own loot.",
    "Battle pass quests count monster kills, opened chests and damage to players. The pass is extended.",
    "Matchmaking as in the original: party size matches the room. Hard uses Skirmish Mode: parties of any size in one battle.",
    "Challenger Mode in the map window works: solos and duos are matched against trios. In a party the captain chooses it.",
    "Launcher lobby: the vote starts once 8 players have joined a tab; the hall waits for 8 players (up to 5 minutes past its timer). Matchmaking rules are shown in launcher 1.5.1.",
    "Leaderboard: average player kills per dungeon battle."
   ],
   "fixed": [
    "The Wendigo treasury door opens after it is killed.",
    "Players no longer fall through the floor at the start of the second floor."
   ],
   "balance": [
    "Removed the +300 health bonus at level 20.",
    "System listings on the auction and Buyout Trades are off: only players trade.",
    "On Hard all gear must be Epic or higher, as in the original.",
    "Forge materials are no longer granted on login."
   ]
  }
 },
 {
  "version": "2026.11.07.2",
  "date": "2026-11-07",
  "ru": {
   "title": "Новый старт плейтеста",
   "new": [
    "Сезон начат заново: прогресс сброшен, новый персонаж получает 10 000 000 золота.",
    "Комнаты больше не делятся строго на соло, дуо и трио: размер, выбранный голосованием, — это наибольшая группа. В комнату трио можно войти трио, дуо или соло, в комнату дуо — дуо или соло, в соло — только соло.",
    "Торговец обновляет товары каждый час.",
    "На Buyout Trades и на аукционе теперь по 25 штук каждого предмета."
   ],
   "fixed": [
    "Предметы больше не ложатся друг на друга, когда меняешь их местами в инвентаре.",
    "Отменённый заказ на покупку возвращает золото: оно ждёт в списке торговли, его можно забрать.",
    "Аукцион открыт всегда."
   ],
   "balance": [
    "На 20 уровне базовое здоровье +300.",
    "Крафтовые реликвии торговец покупает за 0, выставить их на торговлю нельзя."
   ]
  },
  "en": {
   "title": "Playtest restart",
   "new": [
    "The season starts over: progress is reset, a new character gets 10,000,000 gold.",
    "Rooms are no longer strictly solo, duo or trio: the voted size is the largest group allowed. A trio room takes trios, duos or solos, a duo room takes duos or solos, a solo room takes only solos.",
    "The merchant restocks every hour.",
    "Buyout Trades and the auction now have 25 of each item."
   ],
   "fixed": [
    "Items no longer end up on top of each other when you swap them in the inventory.",
    "A cancelled purchase order returns your gold: it waits in the trade list for you to collect.",
    "The auction is always open."
   ],
   "balance": [
    "At level 20 base health is +300.",
    "The merchant buys crafted relics for 0, and they cannot be listed on the trade."
   ]
  }
 },
 {
  "version": "2026.11.07",
  "date": "2026-11-07",
  "ru": {
   "title": "Обновление 7 ноября",
   "new": [
    "Сообщения о красных и фиолетовых порталах.",
    "Снаряжение напарника, которое вынес другой игрок, возвращается владельцу по почте."
   ],
   "fixed": [
    "Значок напарника больше не стоит на вещах всех игроков: теперь он отмечает только снаряжение твоей команды.",
    "Вынесенное из рейда снаряжение больше не продаётся автоматически при выходе.",
    "Исправлена частая причина падений и отказа инвентаря при долгой игре."
   ],
   "balance": [
    "Снято ограничение «Все слоты для оборудования только…» при поиске матча."
   ]
  },
  "en": {
   "title": "Update of 7 November",
   "new": [
    "Messages about red and purple portals.",
    "Gear of a teammate that another player carried out comes back to its owner by mail."
   ],
   "fixed": [
    "The teammate mark no longer sits on everyone's items: it now marks only your team's gear.",
    "Gear carried out of a raid is no longer sold automatically on exit.",
    "Fixed a common cause of crashes and the inventory failing during long play."
   ],
   "balance": [
    "Removed the \"All equipment slots only…\" restriction when searching for a match."
   ]
  }
 }
];
