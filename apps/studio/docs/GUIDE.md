# Balance Studio: guide for designers

*Русская версия — ниже.*

Balance Studio helps you propose balance changes for Dungeonborne. You change values, and the app saves them to a **patch**
file. You send the patch to the server owner, who reviews it and turns it on.

The app changes nothing in the game or on the server and doesn't use the internet. You don't need Python or Excel.

## What you need
- Windows 10 or 11.
- `BalanceStudio-1.1.0.exe`. No installation.
- **A kit**: a zip archive with the tables, from the server owner. Don't unpack it.

Windows may warn that the app is unknown, because the file isn't signed yet. If you got it from the server owner, choose
"More info" and "Run anyway".

## Quick start
1. Start the app and open the kit: use "Open zip" or drop the archive on the window.
2. Change values in any table, or on the "Items" and "Class curves" screens.
3. Open "My changes" and enter your nickname and why you made the change.
4. Press "Save patch" and send the `patch-….json` file to the server owner the way you agreed.

Changes save themselves, so you can close the app at any time.

## Screens
- **Summary.** Kit version, tables and important notes. Read the notes once: they explain what can be changed.
- **My changes.** All changes as "was → now", errors and warnings, undo, saving the patch.
- **Items.** Everything about an item on one screen: properties, durability, vendor prices, player market, dismantling, crafting,
  affixes. Next to min and max you see all the values the item can get.
- **Class curves.** A class attribute by level. Drag a point or edit it in the table below. The dashed line is the value in
  the game.
- **Loot.** What chests, monsters and vases drop on the chosen map: see "Loot" below.
- **Tables.** Every table of the kit by section: items, economy, experience, loot, reference. Search, filters, click a header to sort.

## Editing a table
| Action | How |
| --- | --- |
| Select a cell | click or arrow keys |
| Replace a value | just start typing |
| Edit the current value | F2 or double click |
| Confirm | Enter (down) or Tab (right) |
| Cancel typing | Esc |
| Back to the kit's value | Delete |
| Undo and redo | Ctrl+Z and Ctrl+Y |
| Paste a block from Excel | Ctrl+V: from the selected cell, locked cells are skipped |

Percent values are typed in percent: for 9%, type `9`.

## Bulk edit
For example, to make all potions 20% cheaper:
1. Open "Vendor: prices" and pick the "Consumables" category.
2. Press "Bulk edit" and check the price field.
3. Choose "Percent", type `-20`, check the "was → now" list and press "Apply".

The edit applies to all rows of the filter. To pick particular rows, click their names in the table: Ctrl adds a row, Shift
selects a range. One Ctrl+Z undoes the whole bulk edit.

## Loot
A loot source (chest, monster, vase) rolls each of its **slots** separately. A slot that drops gives **one** item from its **pool**, by weight.

On the left pick a map and a source type, then a group. "No map of its own" means default loot: a source with no row for the current map
uses it. The group's card shows:
- **On average from one source:** how many items and how much gold one chest or monster gives, and what the kit had.
- A slot's **chance:** in percent, 0 to 100. Chance 0 removes the slot.
- An item's **weight:** its share among the pool's items. Weight 0 removes the item, but at least one item in the pool must keep a weight.
- **Count:** how many pieces. With several numbers, one of them drops, evenly.
- **Pool share** and **per source:** the result, with your changes and the season's rules.

Worth knowing:
- **A pool can be shared.** If the card warns that other groups use the pool, changing an item changes them all.
- **Season rules** can remove categories and multiply chances. The fields keep the template's values and the result is shown beside
  them. The rules themselves are in the "Loot: rules and modes" table.
- **Group names** come from the source type and the game's file names, like "Chests: Gold 01, Gold 02 and 8 more".
- **You can't add an item or a slot,** only change existing ones.
- **Drop per whole raid isn't calculated:** the kit doesn't know how many sources a map has.

## Colors in tables
- **Bluish** cell: editable. No highlight: reference only.
- **Yellow:** differs from the original game. The tooltip shows the value in the game.
- **Green:** the game has no such row.
- **Blue:** your change.
- **Red:** an error, the patch can't be saved with it. Yellow frame: a warning, you can still save.

## Errors and warnings
- **A number is required, a whole number is required.** The field doesn't accept such a value.
- **Minimum is above maximum.** Fix the other bound.
- **Changed more than 3 times, far above any value in the game.** You can save, but check for a typo.
- **The range doesn't divide by the step.** An item's value may miss the bounds: pick min and max as multiples of the step.
- **The value is outside the allowed limits.** For example, a chance above 100 or a negative weight.
- **No item would be left in the pool.** Keep a weight above zero on at least one item.

## What can't be changed here
- **Skill damage:** it's set in effects and curves inside the game, not in these tables.
- **What loot consists of:** new items and slots, which sources belong to a group.
- **Which properties an item has:** that's decided when the item is created.
- **Crafting time and market price limits:** the server doesn't use them for now, so they aren't in the kit.

The app also doesn't compute final damage or final hero stats. Curves show table values, without items. Changing min and max
also changes items already dropped.

## A new kit version
The kit is updated along with the server. Open the new archive and the app offers to move your changes:
- **Will move by themselves:** the value on the server didn't change.
- **Already in the new kit:** the change is no longer needed.
- **Needs a decision:** the value on the server changed. Pick yours or the server's.
- **Lost:** the row or field is gone. These changes stay in the old draft.

A patch is tied to a kit version. If the server was updated, open the new kit, move your changes and save the patch again.

## A patch from an LLM or Excel
You can still edit through `balance.xlsx` or ask ChatGPT or Claude: the instructions are in the kit archive. Open such a patch
with "Open patch" on "My changes". The app checks it and shows what fit.

## If something goes wrong
- **"This is not a balance kit".** Ask the server owner for the whole archive, not unpacked and not rebuilt.
- **"This file doesn't fit"** when opening a patch: it isn't a patch, or it's for another season.
- **The server refused the patch.** Most likely the tables have changed: open the new kit and save the patch again.

The app's data is in `%LOCALAPPDATA%\BalanceStudio`: copies of kits and drafts.

---

# Balance Studio: инструкция для дизайнеров

Balance Studio помогает предложить изменения баланса Dungeonborne. Вы правите значения, приложение сохраняет их в файл
**патча**. Патч вы отправляете владельцу сервера, он проверяет его и включает на сервере.

Приложение ничего не меняет в игре и на сервере и не ходит в интернет. Python и Excel не нужны.

## Что нужно
- Windows 10 или 11.
- Файл `BalanceStudio-1.1.0.exe`. Устанавливать не надо.
- **Набор**: zip-архив с таблицами, его выдаёт владелец сервера. Распаковывать не надо.

Windows может предупредить, что приложение неизвестно: файл пока не подписан. Если вы получили его от владельца сервера,
нажмите «Подробнее» и «Выполнить в любом случае».

## Быстрый старт
1. Запустите приложение и откройте набор: кнопкой «Открыть zip» или перетащите архив в окно.
2. Измените значения в любой таблице или на экранах «Предметы» и «Кривые классов».
3. Откройте «Мои правки», укажите ник и зачем нужна правка.
4. Нажмите «Сохранить патч» и отправьте файл `patch-….json` владельцу сервера тем способом, о котором вы договорились.

Правки сохраняются сами: приложение можно закрыть в любой момент.

## Экраны
- **Сводка.** Версия набора, таблицы и важные оговорки. Прочитайте оговорки один раз: они объясняют, что можно изменить.
- **Мои правки.** Все изменения «было → стало», ошибки и предупреждения, откат, сохранение патча.
- **Предметы.** Всё о предмете на одном экране: свойства, прочность, цены у торговца, рынок игроков, разбор, крафт, аффиксы.
  Рядом с мин. и макс. видны все значения, которые может получить предмет.
- **Кривые классов.** Атрибут класса по уровням. Точку можно тянуть мышью или править в таблице ниже. Пунктир показывает
  значение в игре.
- **Лут.** Что падает с сундуков, монстров и ваз на выбранной карте: см. раздел «Лут» ниже.
- **Таблицы.** Все таблицы набора по разделам: предметы, экономика, опыт, лут, справка. Поиск, фильтры, сортировка по клику на
  заголовок.

## Правка в таблице
| Действие | Как |
| --- | --- |
| Выбрать ячейку | клик или стрелки |
| Заменить значение | просто начните печатать |
| Изменить текущее значение | F2 или двойной клик |
| Подтвердить | Enter (вниз) или Tab (вправо) |
| Отменить ввод | Esc |
| Вернуть значение из набора | Delete |
| Отменить и повторить правку | Ctrl+Z и Ctrl+Y |
| Вставить блок из Excel | Ctrl+V: от выбранной ячейки, закрытые ячейки пропускаются |

Значения в процентах вводятся в процентах: чтобы получить 9 %, введите `9`.

## Массовая правка
Например, сделать все зелья на 20 % дешевле:
1. Откройте таблицу «Торговец: цены», выберите категорию «Расходники».
2. Нажмите «Массовая правка» и отметьте поле с ценой.
3. Выберите «Проценты», введите `-20`, проверьте список «было → стало» и нажмите «Применить».

Правка действует на все строки фильтра. Чтобы выбрать отдельные строки, щёлкайте по их названиям в таблице: Ctrl добавляет
строку, Shift выделяет диапазон. Массовая правка отменяется одним нажатием Ctrl+Z.

## Лут
Источник лута (сундук, монстр, ваза) бросает каждый свой **слот** отдельно. Выпавший слот даёт **один** предмет из **пула** по весам.

Слева выберите карту и тип источника, затем группу. Строка «Без своей карты» значит лут по умолчанию: его берёт источник, у которого нет
строки для текущей карты. В карточке группы:
- **В среднем с одного источника:** сколько предметов и золота даёт один сундук или монстр, и что было в наборе.
- **Шанс** слота: в процентах, от 0 до 100. Шанс 0 убирает слот.
- **Вес** предмета: его доля среди предметов пула. Вес 0 убирает предмет, но хотя бы у одного предмета пула вес должен остаться.
- **Кол-во:** сколько штук. Если чисел несколько, выпадает одно из них, поровну.
- **Доля в пуле** и **с источника:** что получается в итоге, с учётом ваших правок и правил сезона.

Что важно знать:
- **Пул бывает общим.** Если карточка предупреждает, что пул используют другие группы, правка предмета изменит их все.
- **Правила сезона** могут убирать категории и умножать шансы. В полях остаются значения шаблона, а итог виден рядом. Сами правила
  правятся в таблице «Лут: правила и режимы».
- **Названия групп** собраны из типа источника и имён файлов игры, например «Сундуки: Gold 01, Gold 02 и ещё 8».
- **Добавить предмет или слот нельзя,** только изменить существующие.
- **Дроп за весь рейд не считается:** набор не знает, сколько источников стоит на карте.

## Цвета в таблицах
- **Голубоватая** ячейка: можно править. Без подсветки: только справка.
- **Жёлтая:** отличается от оригинальной игры. Подсказка над ячейкой показывает значение в игре.
- **Зелёная:** такой строки в игре нет.
- **Синяя:** ваша правка.
- **Красная:** ошибка, с ней патч не сохранить. Жёлтая рамка: предупреждение, сохранить можно.

## Ошибки и предупреждения
- **Нужно число, нужно целое число.** Поле не принимает такое значение.
- **Минимум больше максимума.** Поправьте вторую границу.
- **Изменение больше чем в 3 раза, намного больше любого значения в игре.** Сохранить можно, но проверьте, нет ли опечатки.
- **Диапазон не делится на шаг.** Значение предмета может не попасть в границы: подберите мин. и макс. кратно шагу.
- **Значение вне допустимых границ.** Например, шанс больше 100 или отрицательный вес.
- **В пуле не останется ни одного предмета.** Оставьте хотя бы у одного предмета вес больше нуля.

## Что здесь нельзя изменить
- **Урон умений:** он задан в эффектах и кривых внутри игры, а не в этих таблицах.
- **Состав лута:** новые предметы и слоты, какие источники входят в группу.
- **Какие свойства есть у предмета:** это решается при его создании.
- **Время крафта и лимиты цен на рынке:** сервер их сейчас не использует, поэтому в наборе их нет.

Также приложение не считает итоговый урон и итоговые статы героя. Кривые показывают значения таблицы, без предметов.
Правка мин. и макс. меняет и уже выпавшие предметы.

## Новая версия набора
Набор обновляется вместе с сервером. Откройте новый архив, и приложение предложит перенести правки:
- **Перенесутся сами:** значение на сервере не менялось.
- **Уже есть в новом наборе:** правка больше не нужна.
- **Нужно решить:** значение на сервере изменилось. Выберите своё или серверное.
- **Потеряны:** строки или поля больше нет. Эти правки останутся в старом черновике.

Патч привязан к версии набора. Если сервер обновился, откройте новый набор, перенесите правки и сохраните патч заново.

## Патч от LLM или из Excel
Набор по-прежнему можно править через `balance.xlsx` или попросить ChatGPT или Claude: инструкция в архиве набора. Такой
патч можно открыть кнопкой «Открыть патч» в «Моих правках». Приложение проверит его и покажет, что подошло.

## Если что-то пошло не так
- **«Это не набор баланса».** Попросите у владельца сервера архив целиком, не распакованный и не пересобранный.
- **«Этот файл не подходит»** при открытии патча: это не патч, или он для другого сезона.
- **Сервер не принял патч.** Скорее всего, таблицы успели измениться: откройте новый набор и сохраните патч заново.

Данные приложения лежат в `%LOCALAPPDATA%\BalanceStudio`: копии наборов и черновики.
