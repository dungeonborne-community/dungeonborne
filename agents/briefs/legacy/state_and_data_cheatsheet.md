# 📘 Cheat sheet for the `state` (Player) and `data` (DUserRole) fields

## 📄 Structure of the `state.json` file

| Field path (JSON / Rust) | Data type | Example value (what it should be) |
| :--- | :--- | :--- |
| `open_id` | `str` | `"76561198000000904"` |
| `user_id` | `int` | `10001` |
| `next_guid` | `int` | `1000002` |
| `next_item_guid` | `int` | `100000124` |
| `hall_role_guid` | `int` | `1000001` |
| `roles` | `null` | `null` |
| `player_setting.data.113.mutable_value.repeated_value[]` | `Array<str>` | `["1", "1", "0", ...]` |
| `player_setting.data.121.mutable_value.repeated_value[]` | `Array<str>` | `["1000001"]` |
| `player_setting.data.125.mutable_value.repeated_value[]` | `Array<str>` | `["1000001"]` |
| `player_setting.data.126.mutable_value.repeated_value[]` | `Array<str>` | `["1000001"]` |
| `player_setting.data.127.mutable_value.repeated_value[]` | `Array<str>` | `["1000001"]` |
| `player_setting.data.129.mutable_value.repeated_value[]` | `Array<str>` | `["1000001"]` |
| `level_select.level_type` | `int` | `2001` |
| `level_select.level_id` | `int` | `40003020` |
| `level_select.region` | `str` | `"de"` |
| `inventory.coins_data[].coin_type_id` | `int` | `1052` |
| `inventory.coins_data[].count` | `str` | `"999"` |
| `inventory.inventory_pages[].column_size` | `int` | `10` |
| `inventory.inventory_pages[].item_group[].element[].basic.count` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].basic.guid` | `str` | `"100000074"` |
| `inventory.inventory_pages[].item_group[].element[].basic.position.type` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].basic.position.y` | `int` | `9` |
| `inventory.inventory_pages[].item_group[].element[].basic.type_id` | `int` | `185016000` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].affix_type` | `str` | `"EN_BATTLE_AFFIX_TYPE_LEGENDARY"` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1101.random_data[].random_level` | `int` | `5525` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].location` | `str` | `"EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_EQUIP"` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1001.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1001.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1001.random_data[].random_level` | `int` | `3337` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1021.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1021.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1021.random_data[].random_level` | `int` | `3244` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1103.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1103.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1103.random_data[].random_level` | `int` | `1990` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.4001.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.4001.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.4001.random_data[].random_level` | `int` | `6145` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1118.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1118.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1118.random_data[].random_level` | `int` | `1905` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.189010001.random_data[].legendary_affix_id` | `int` | `189010001` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.189010001.random_data[].random_level` | `int` | `5133` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2008.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2008.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2008.random_data[].random_level` | `int` | `6980` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].gear_set_random.11008.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].gear_set_random.11008.random_data[].random_level` | `int` | `9566` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1104.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1104.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1104.random_data[].random_level` | `int` | `9030` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6212.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6212.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6212.random_data[].random_level` | `int` | `1740` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2002.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2002.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2002.random_data[].random_level` | `int` | `8765` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.185016002.random_data[].legendary_affix_id` | `int` | `185016002` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.185016002.random_data[].random_level` | `int` | `9361` |
| `inventory.inventory_pages[].item_group[].element[].basic.position.x` | `int` | `4` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].gear_set_random.11001.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].gear_set_random.11001.random_data[].random_level` | `int` | `226` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1002.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1002.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1002.random_data[].random_level` | `int` | `298` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1117.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1117.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1117.random_data[].random_level` | `int` | `7702` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1005.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1005.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1005.random_data[].random_level` | `int` | `8543` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.185016001.random_data[].legendary_affix_id` | `int` | `185016001` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.185016001.random_data[].random_level` | `int` | `7999` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1004.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1004.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1004.random_data[].random_level` | `int` | `1566` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1107.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1107.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1107.random_data[].random_level` | `int` | `3018` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6213.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6213.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6213.random_data[].random_level` | `int` | `4998` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1110.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1110.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1110.random_data[].random_level` | `int` | `7347` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181055003.random_data[].legendary_affix_id` | `int` | `181055003` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181055003.random_data[].random_level` | `int` | `4092` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1102.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1102.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1102.random_data[].random_level` | `int` | `7511` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181055001.random_data[].legendary_affix_id` | `int` | `181055001` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181055001.random_data[].random_level` | `int` | `2280` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6211.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6211.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6211.random_data[].random_level` | `int` | `120` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181016000.random_data[].legendary_affix_id` | `int` | `181016000` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181016000.random_data[].random_level` | `int` | `1237` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6210.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6210.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.6210.random_data[].random_level` | `int` | `6586` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181085000.random_data[].legendary_affix_id` | `int` | `181085000` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181085000.random_data[].random_level` | `int` | `8624` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].gear_set_random.11006.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].gear_set_random.11006.random_data[].random_level` | `int` | `4988` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1006.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1006.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1006.random_data[].random_level` | `int` | `1846` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.185015000.random_data[].legendary_affix_id` | `int` | `185015000` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.185015000.random_data[].random_level` | `int` | `1875` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181015000.random_data[].legendary_affix_id` | `int` | `181015000` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181015000.random_data[].random_level` | `int` | `6632` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1105.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1105.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1105.random_data[].random_level` | `int` | `7384` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1114.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1114.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1114.random_data[].random_level` | `int` | `7683` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181045000.random_data[].legendary_affix_id` | `int` | `181045000` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181045000.random_data[].random_level` | `int` | `9796` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181056003.random_data[].legendary_affix_id` | `int` | `181056003` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181056003.random_data[].random_level` | `int` | `72` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181026001.random_data[].legendary_affix_id` | `int` | `181026001` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181026001.random_data[].random_level` | `int` | `8439` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181025001.random_data[].legendary_affix_id` | `int` | `181025001` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181025001.random_data[].random_level` | `int` | `1196` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1003.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1003.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1003.random_data[].random_level` | `int` | `4183` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2007.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2007.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2007.random_data[].random_level` | `int` | `3051` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1119.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1119.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.1119.random_data[].random_level` | `int` | `6715` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181035001.random_data[].legendary_affix_id` | `int` | `181035001` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.181035001.random_data[].random_level` | `int` | `5540` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].gear_set_random.11003.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].gear_set_random.11003.random_data[].random_level` | `int` | `1208` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2012.random_data[].affix_pool_value` | `bool` | `true` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2012.random_data[].group_index` | `int` | `1` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].attribute_random.2012.random_data[].random_level` | `int` | `6591` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.185016000.random_data[].legendary_affix_id` | `int` | `185016000` |
| `inventory.inventory_pages[].item_group[].element[].data.affix_data[].ability_set_random.185016000.random_data[].random_level` | `int` | `8715` |
| `inventory.inventory_pages[].page_id` | `str` | `"7"` |
| `inventory.inventory_pages[].row_size` | `int` | `17` |
| `starter_given[]` | `Array<int>` | `[1000001]` |
| `market.tabs.1009.gen` | `int` | `4` |
| `market.tabs.1009.period` | `int` | `1266` |
| `market.tabs.1009.data.available_products[].available_count` | `int` | `1` |
| `market.tabs.1009.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `1440` |
| `market.tabs.1009.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.1009.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `1440` |
| `market.tabs.1009.data.available_products[].identified` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].basic.count` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].basic.guid` | `str` | `"100000048"` |
| `market.tabs.1009.data.available_products[].identified_items[].basic.type_id` | `int` | `185012004` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].affix_type` | `int` | `20` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1101.random_data[].random_level` | `int` | `7425` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].location` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].random_level` | `int` | `3127` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].random_level` | `int` | `6511` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6102.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6102.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6102.random_data[].random_level` | `int` | `9678` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6214.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6214.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6214.random_data[].random_level` | `int` | `7097` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6210.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6210.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6210.random_data[].random_level` | `int` | `9681` |
| `market.tabs.1009.data.available_products[].last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1009.data.available_products[].next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1009.data.available_products[].product_id` | `int` | `760090` |
| `market.tabs.1009.data.available_products[].total_available_count` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].random_level` | `int` | `3949` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].random_level` | `int` | `9441` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.4004.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.4004.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.4004.random_data[].random_level` | `int` | `9226` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1114.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1114.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1114.random_data[].random_level` | `int` | `1376` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1103.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1103.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1103.random_data[].random_level` | `int` | `5314` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2002.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2002.random_data[].random_level` | `int` | `7901` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1106.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1106.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1106.random_data[].random_level` | `int` | `7650` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1102.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1102.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1102.random_data[].random_level` | `int` | `2618` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1006.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1006.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1006.random_data[].random_level` | `int` | `5218` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].random_level` | `int` | `2472` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2008.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2008.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2008.random_data[].random_level` | `int` | `9757` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].random_level` | `int` | `8934` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1116.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1116.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1116.random_data[].random_level` | `int` | `112` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1005.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1005.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1005.random_data[].random_level` | `int` | `3114` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1118.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1118.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1118.random_data[].random_level` | `int` | `7888` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6213.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6213.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6213.random_data[].random_level` | `int` | `8813` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1117.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1117.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1117.random_data[].random_level` | `int` | `4174` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6211.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6211.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6211.random_data[].random_level` | `int` | `9299` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6212.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6212.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.6212.random_data[].random_level` | `int` | `1224` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.4001.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.4001.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.4001.random_data[].random_level` | `int` | `5866` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1105.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1105.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1105.random_data[].random_level` | `int` | `5989` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1121.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1121.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1121.random_data[].random_level` | `int` | `6066` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1113.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1113.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1113.random_data[].random_level` | `int` | `6496` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2010.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2010.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2010.random_data[].random_level` | `int` | `9152` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1115.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1115.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.1115.random_data[].random_level` | `int` | `3661` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].random_level` | `int` | `9180` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].gear_set_random.11009.random_data[].group_index` | `int` | `1` |
| `market.tabs.1009.data.available_products[].identified_items[].data.affix_data[].gear_set_random.11009.random_data[].random_level` | `int` | `710` |
| `market.tabs.1009.data.last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1009.data.market_tab_id` | `int` | `1009` |
| `market.tabs.1009.data.next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1010.gen` | `int` | `4` |
| `market.tabs.1010.period` | `int` | `1266` |
| `market.tabs.1010.data.available_products[].available_count` | `int` | `1` |
| `market.tabs.1010.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `36000` |
| `market.tabs.1010.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.1010.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `36000` |
| `market.tabs.1010.data.available_products[].identified` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].basic.count` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].basic.guid` | `str` | `"100000058"` |
| `market.tabs.1010.data.available_products[].identified_items[].basic.type_id` | `int` | `182044000` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].affix_type` | `int` | `20` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1105.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1105.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1105.random_data[].random_level` | `int` | `6456` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2002.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2002.random_data[].random_level` | `int` | `8156` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].location` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].random_level` | `int` | `743` |
| `market.tabs.1010.data.available_products[].last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1010.data.available_products[].next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1010.data.available_products[].product_id` | `int` | `751230` |
| `market.tabs.1010.data.available_products[].total_available_count` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].random_level` | `int` | `8395` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1005.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1005.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1005.random_data[].random_level` | `int` | `5239` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2010.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2010.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2010.random_data[].random_level` | `int` | `1038` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].random_level` | `int` | `6610` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2009.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2009.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2009.random_data[].random_level` | `int` | `3882` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].random_level` | `int` | `4101` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2012.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2012.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2012.random_data[].random_level` | `int` | `3055` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].random_level` | `int` | `4289` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].random_level` | `int` | `7712` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.4002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.4002.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.4002.random_data[].random_level` | `int` | `1570` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.4004.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.4004.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.4004.random_data[].random_level` | `int` | `1927` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.5006.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.5006.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.5006.random_data[].random_level` | `int` | `7165` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2011.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2011.random_data[].group_index` | `int` | `1` |
| `market.tabs.1010.data.available_products[].identified_items[].data.affix_data[].attribute_random.2011.random_data[].random_level` | `int` | `3570` |
| `market.tabs.1010.data.last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1010.data.market_tab_id` | `int` | `1010` |
| `market.tabs.1010.data.next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1011.gen` | `int` | `4` |
| `market.tabs.1011.period` | `int` | `1266` |
| `market.tabs.1011.data.available_products[].available_count` | `int` | `1` |
| `market.tabs.1011.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `24000` |
| `market.tabs.1011.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.1011.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `24000` |
| `market.tabs.1011.data.available_products[].identified` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].basic.count` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].basic.guid` | `str` | `"100000068"` |
| `market.tabs.1011.data.available_products[].identified_items[].basic.type_id` | `int` | `185024003` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].affix_type` | `int` | `20` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1003.random_data[].random_level` | `int` | `445` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2008.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2008.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2008.random_data[].random_level` | `int` | `3881` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].location` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1001.random_data[].random_level` | `int` | `7597` |
| `market.tabs.1011.data.available_products[].last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1011.data.available_products[].next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1011.data.available_products[].product_id` | `int` | `751330` |
| `market.tabs.1011.data.available_products[].total_available_count` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].random_level` | `int` | `1586` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1113.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1113.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1113.random_data[].random_level` | `int` | `816` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1103.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1103.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1103.random_data[].random_level` | `int` | `8022` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2003.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2003.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2003.random_data[].random_level` | `int` | `4704` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1006.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1006.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1006.random_data[].random_level` | `int` | `7562` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1115.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1115.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1115.random_data[].random_level` | `int` | `2458` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1004.random_data[].random_level` | `int` | `7602` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].random_level` | `int` | `5790` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].gear_set_random.11009.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].gear_set_random.11009.random_data[].random_level` | `int` | `4319` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.6213.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.6213.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.6213.random_data[].random_level` | `int` | `8813` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2007.random_data[].random_level` | `int` | `5936` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2005.random_data[].random_level` | `int` | `4798` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2011.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2011.random_data[].group_index` | `int` | `1` |
| `market.tabs.1011.data.available_products[].identified_items[].data.affix_data[].attribute_random.2011.random_data[].random_level` | `int` | `2664` |
| `market.tabs.1011.data.last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1011.data.market_tab_id` | `int` | `1011` |
| `market.tabs.1011.data.next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1014.gen` | `int` | `4` |
| `market.tabs.1014.period` | `int` | `1266` |
| `market.tabs.1014.data.available_products[].available_count` | `int` | `999` |
| `market.tabs.1014.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `1600` |
| `market.tabs.1014.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.1014.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `1600` |
| `market.tabs.1014.data.available_products[].last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1014.data.available_products[].next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1014.data.available_products[].product_id` | `int` | `751419` |
| `market.tabs.1014.data.available_products[].total_available_count` | `int` | `999` |
| `market.tabs.1014.data.last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1014.data.market_tab_id` | `int` | `1014` |
| `market.tabs.1014.data.next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1015.gen` | `int` | `4` |
| `market.tabs.1015.period` | `int` | `1266` |
| `market.tabs.1015.data.available_products[].available_count` | `int` | `999` |
| `market.tabs.1015.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `10800` |
| `market.tabs.1015.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.1015.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `10800` |
| `market.tabs.1015.data.available_products[].last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1015.data.available_products[].next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1015.data.available_products[].product_id` | `int` | `751522` |
| `market.tabs.1015.data.available_products[].total_available_count` | `int` | `999` |
| `market.tabs.1015.data.last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1015.data.market_tab_id` | `int` | `1015` |
| `market.tabs.1015.data.next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1016.gen` | `int` | `4` |
| `market.tabs.1016.period` | `int` | `1266` |
| `market.tabs.1016.data.available_products[].available_count` | `int` | `999` |
| `market.tabs.1016.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `25` |
| `market.tabs.1016.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.1016.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `25` |
| `market.tabs.1016.data.available_products[].last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1016.data.available_products[].next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1016.data.available_products[].product_id` | `int` | `705012` |
| `market.tabs.1016.data.available_products[].total_available_count` | `int` | `999` |
| `market.tabs.1016.data.available_products[].identified` | `bool` | `true` |
| `market.tabs.1016.data.available_products[].identified_items[].basic.count` | `int` | `1` |
| `market.tabs.1016.data.available_products[].identified_items[].basic.guid` | `str` | `"100000069"` |
| `market.tabs.1016.data.available_products[].identified_items[].basic.type_id` | `int` | `181010000` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].affix_type` | `int` | `10` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].attribute_random.1101.random_data[].random_level` | `int` | `717` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].location` | `int` | `1` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].group_index` | `int` | `1` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].attribute_random.1002.random_data[].random_level` | `int` | `4880` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].affix_pool_value` | `bool` | `true` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].group_index` | `int` | `1` |
| `market.tabs.1016.data.available_products[].identified_items[].data.affix_data[].attribute_random.1104.random_data[].random_level` | `int` | `8661` |
| `market.tabs.1016.data.last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1016.data.market_tab_id` | `int` | `1016` |
| `market.tabs.1016.data.next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1017.gen` | `int` | `4` |
| `market.tabs.1017.period` | `int` | `1266` |
| `market.tabs.1017.data.available_products[].available_count` | `int` | `0` |
| `market.tabs.1017.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `2700` |
| `market.tabs.1017.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.1017.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `2700` |
| `market.tabs.1017.data.available_products[].last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1017.data.available_products[].next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1017.data.available_products[].product_id` | `int` | `706006` |
| `market.tabs.1017.data.available_products[].total_available_count` | `int` | `999` |
| `market.tabs.1017.data.available_products[].used_count` | `int` | `999` |
| `market.tabs.1017.data.last_auto_refresh_reset_time` | `str` | `"1792713600"` |
| `market.tabs.1017.data.market_tab_id` | `int` | `1017` |
| `market.tabs.1017.data.next_auto_refresh_reset_time` | `str` | `"1792800000"` |
| `market.tabs.1019.gen` | `int` | `4` |
| `market.tabs.1019.period` | `int` | `30396` |
| `market.tabs.1019.data.available_products[].available_count` | `int` | `999` |
| `market.tabs.1019.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `10000` |
| `market.tabs.1019.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.1019.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `10000` |
| `market.tabs.1019.data.available_products[].last_auto_refresh_reset_time` | `str` | `"1792756800"` |
| `market.tabs.1019.data.available_products[].next_auto_refresh_reset_time` | `str` | `"1792760400"` |
| `market.tabs.1019.data.available_products[].product_id` | `int` | `705020` |
| `market.tabs.1019.data.available_products[].total_available_count` | `int` | `999` |
| `market.tabs.1019.data.last_auto_refresh_reset_time` | `str` | `"1792756800"` |
| `market.tabs.1019.data.market_tab_id` | `int` | `1019` |
| `market.tabs.1019.data.next_auto_refresh_reset_time` | `str` | `"1792760400"` |
| `market.tabs.2000.gen` | `int` | `4` |
| `market.tabs.2000.period` | `int` | `0` |
| `market.tabs.2000.data.available_products[].available_count` | `int` | `999` |
| `market.tabs.2000.data.available_products[].cost_options[].cost_items[].cost_price` | `int` | `1500` |
| `market.tabs.2000.data.available_products[].cost_options[].cost_items[].cost_type` | `int` | `1001` |
| `market.tabs.2000.data.available_products[].cost_options[].cost_items[].origin_price` | `int` | `1500` |
| `market.tabs.2000.data.available_products[].last_auto_refresh_reset_time` | `str` | `"0"` |
| `market.tabs.2000.data.available_products[].next_auto_refresh_reset_time` | `str` | `"0"` |
| `market.tabs.2000.data.available_products[].product_id` | `int` | `720003` |
| `market.tabs.2000.data.available_products[].total_available_count` | `int` | `999` |
| `market.tabs.2000.data.available_products[].used_count` | `int` | `7` |
| `market.tabs.2000.data.last_auto_refresh_reset_time` | `str` | `"0"` |
| `market.tabs.2000.data.market_tab_id` | `int` | `2000` |
| `market.tabs.2000.data.next_auto_refresh_reset_time` | `str` | `"0"` |
| `market.sold[].item_data.basic.count` | `int` | `1` |
| `market.sold[].item_data.basic.guid` | `str` | `"100000108"` |
| `market.sold[].item_data.basic.type_id` | `int` | `220140280` |
| `market.sold[].item_data.data.affix_data[].affix_type` | `str` | `"EN_BATTLE_AFFIX_TYPE_LEGENDARY"` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1103.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1103.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1103.random_data[].random_level` | `int` | `1971` |
| `market.sold[].item_data.data.affix_data[].attribute_random.2002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.2002.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.2002.random_data[].random_level` | `int` | `6243` |
| `market.sold[].item_data.data.affix_data[].location` | `str` | `"EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_HOLD"` |
| `market.sold[].last_sold_timepoint` | `str` | `"1792756686"` |
| `market.sold[].sold_version` | `str` | `"24"` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1106.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1106.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1106.random_data[].random_level` | `int` | `7435` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1101.random_data[].random_level` | `int` | `4138` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1001.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1001.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1001.random_data[].random_level` | `int` | `8531` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1021.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1021.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1021.random_data[].random_level` | `int` | `3114` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1102.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1102.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1102.random_data[].random_level` | `int` | `4366` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1115.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1115.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1115.random_data[].random_level` | `int` | `1325` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181025001.random_data[].legendary_affix_id` | `int` | `181025001` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181025001.random_data[].random_level` | `int` | `3922` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1003.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1003.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1003.random_data[].random_level` | `int` | `3872` |
| `market.sold[].item_data.data.affix_data[].attribute_random.2007.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.2007.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.2007.random_data[].random_level` | `int` | `7499` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1105.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1105.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1105.random_data[].random_level` | `int` | `4831` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1104.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1104.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1104.random_data[].random_level` | `int` | `5420` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181036001.random_data[].legendary_affix_id` | `int` | `181036001` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181036001.random_data[].random_level` | `int` | `255` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1002.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1002.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1002.random_data[].random_level` | `int` | `89` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1118.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1118.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1118.random_data[].random_level` | `int` | `1474` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181015001.random_data[].legendary_affix_id` | `int` | `181015001` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181015001.random_data[].random_level` | `int` | `2746` |
| `market.sold[].item_data.data.affix_data[].attribute_random.6212.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.6212.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.6212.random_data[].random_level` | `int` | `4374` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1117.random_data[].affix_pool_value` | `bool` | `true` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1117.random_data[].group_index` | `int` | `1` |
| `market.sold[].item_data.data.affix_data[].attribute_random.1117.random_data[].random_level` | `int` | `2411` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181025000.random_data[].legendary_affix_id` | `int` | `181025000` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181025000.random_data[].random_level` | `int` | `3944` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181016000.random_data[].legendary_affix_id` | `int` | `181016000` |
| `market.sold[].item_data.data.affix_data[].ability_set_random.181016000.random_data[].random_level` | `int` | `6287` |
| `market.sold[].item_data.create_info.create_timestamp` | `str` | `"2026-10-23T11:57:37Z"` |
| `market.sold[].item_data.create_info.heirloom.owner.user_id` | `str` | `"10001"` |
| `market.sold[].item_data.create_info.heirloom.owner.zone_id` | `int` | `2200` |
| `market.sold[].item_data.data` | `Object (empty)` | `{}` |
| `market.sold[].item_data.data.gem_data.slot_data[].affix_location` | `str` | `"EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_EQUIP"` |
| `market.sold[].item_data.data.gem_data.slot_data[].gem_id` | `int` | `240192000` |
| `market.sold[].item_data.data.gem_data.slot_data[].slot_id` | `int` | `1` |
| `market.sold_version` | `int` | `24` |
| `affix_gen` | `int` | `2` |
| `quest.act` | `int` | `16518` |
| `quest.join` | `int` | `1792755843` |
| `quest.quests.1601000.status` | `int` | `1` |
| `quest.quests.1601000.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1601000.created` | `int` | `1792755843` |
| `quest.quests.1651809.status` | `int` | `1` |
| `quest.quests.1651809.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651809.created` | `int` | `1792755843` |
| `quest.quests.1651812.status` | `int` | `1` |
| `quest.quests.1651812.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651812.created` | `int` | `1792755843` |
| `quest.quests.1651814.status` | `int` | `1` |
| `quest.quests.1651814.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651814.created` | `int` | `1792755843` |
| `quest.quests.1651818.status` | `int` | `1` |
| `quest.quests.1651818.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651818.created` | `int` | `1792755843` |
| `quest.quests.1651818.period` | `int` | `125` |
| `quest.quests.1651819.status` | `int` | `1` |
| `quest.quests.1651819.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651819.created` | `int` | `1792755843` |
| `quest.quests.1651819.period` | `int` | `125` |
| `quest.quests.1651820.status` | `int` | `1` |
| `quest.quests.1651820.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651820.created` | `int` | `1792755843` |
| `quest.quests.1651820.period` | `int` | `125` |
| `quest.quests.1651821.status` | `int` | `1` |
| `quest.quests.1651821.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651821.created` | `int` | `1792755843` |
| `quest.quests.1651821.period` | `int` | `125` |
| `quest.quests.1651822.status` | `int` | `1` |
| `quest.quests.1651822.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651822.created` | `int` | `1792755843` |
| `quest.quests.1651822.period` | `int` | `125` |
| `quest.quests.1651823.status` | `int` | `1` |
| `quest.quests.1651823.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651823.created` | `int` | `1792755843` |
| `quest.quests.1651823.period` | `int` | `125` |
| `quest.quests.1651824.status` | `int` | `1` |
| `quest.quests.1651824.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651824.created` | `int` | `1792755843` |
| `quest.quests.1651824.period` | `int` | `125` |
| `quest.quests.1651825.status` | `int` | `1` |
| `quest.quests.1651825.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651825.created` | `int` | `1792755843` |
| `quest.quests.1651825.period` | `int` | `125` |
| `quest.quests.1651826.status` | `int` | `1` |
| `quest.quests.1651826.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651826.created` | `int` | `1792755843` |
| `quest.quests.1651826.period` | `int` | `125` |
| `quest.quests.1651827.status` | `int` | `1` |
| `quest.quests.1651827.values[]` | `Array<int>` | `[0]` |
| `quest.quests.1651827.created` | `int` | `1792755843` |
| `quest.quests.1651827.period` | `int` | `125` |
| `quest.daily[].quest` | `int` | `1651814` |
| `quest.daily[].idx` | `int` | `3` |
| `quest.daily_idx` | `int` | `3` |
| `quest.daily_day` | `int` | `20749` |
| `quest.daily_at` | `int` | `1792755843` |
| `quest.event_id` | `int` | `1` |
| `quest.dispatched` | `int` | `3` |
| `heirloom.unlocked[]` | `Array<int>` | `[210140220, 210140230, 210140240, ...]` |
| `heirloom.gems[]` | `Array<int>` | `[230192000, 230192001, 230192002, ...]` |
| `heirloom.inlaid.21014020[].slot` | `int` | `3` |
| `heirloom.inlaid.21014020[].gem` | `int` | `240184000` |
| `heirloom.inlaid.21014020[].loc` | `str` | `"EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_EQUIP"` |
| `raid_done` | `str` | `"6adb48b4bf76-11"` |

<details><summary><b>Show a compact JSON template (1 element per array)</b></summary>

```json
{
  "open_id": "76561198000000904",
  "user_id": 10001,
  "next_guid": 1000002,
  "next_item_guid": 100000124,
  "hall_role_guid": 1000001,
  "roles": null,
  "player_setting": {
    "data": {
      "113": {
        "mutable_value": {
          "repeated_value": [
            "1"
          ]
        }
      },
      "121": {
        "mutable_value": {
          "repeated_value": [
            "1000001"
          ]
        }
      },
      "125": {
        "mutable_value": {
          "repeated_value": [
            "1000001"
          ]
        }
      },
      "126": {
        "mutable_value": {
          "repeated_value": [
            "1000001"
          ]
        }
      },
      "127": {
        "mutable_value": {
          "repeated_value": [
            "1000001"
          ]
        }
      },
      "129": {
        "mutable_value": {
          "repeated_value": [
            "1000001"
          ]
        }
      }
    }
  },
  "level_select": {
    "level_type": 2001,
    "level_id": 40003020,
    "region": "de"
  },
  "inventory": {
    "coins_data": [
      {
        "coin_type_id": 1001,
        "count": "972200"
      }
    ],
    "inventory_pages": [
      {
        "column_size": 10,
        "item_group": [
          {
            "element": [
              {
                "basic": {
                  "count": 1,
                  "guid": "100000070",
                  "position": {
                    "type": 1,
                    "y": 2
                  },
                  "type_id": 189010001
                },
                "data": {
                  "affix_data": [
                    {
                      "affix_type": "EN_BATTLE_AFFIX_TYPE_BASIC",
                      "attribute_random": {
                        "1101": {
                          "random_data": [
                            {
                              "random_level": 6472
                            }
                          ]
                        }
                      },
                      "location": "EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_HOLD"
                    }
                  ]
                }
              }
            ]
          }
        ],
        "page_id": "0",
        "row_size": 17
      }
    ]
  },
  "starter_given": [
    1000001
  ],
  "market": {
    "tabs": {
      "1009": {
        "gen": 4,
        "period": 1266,
        "data": {
          "available_products": [
            {
              "available_count": 1,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 27000,
                      "cost_type": 1001,
                      "origin_price": 27000
                    }
                  ]
                }
              ],
              "identified": true,
              "identified_items": [
                {
                  "basic": {
                    "count": 1,
                    "guid": "100000019",
                    "type_id": 181014000
                  },
                  "data": {
                    "affix_data": [
                      {
                        "affix_type": 5,
                        "attribute_random": {
                          "1101": {
                            "random_data": [
                              {
                                "random_level": 9724
                              }
                            ]
                          }
                        },
                        "location": 3
                      }
                    ]
                  }
                }
              ],
              "last_auto_refresh_reset_time": "1792713600",
              "next_auto_refresh_reset_time": "1792800000",
              "product_id": 760061,
              "total_available_count": 1
            }
          ],
          "last_auto_refresh_reset_time": "1792713600",
          "market_tab_id": 1009,
          "next_auto_refresh_reset_time": "1792800000"
        }
      },
      "1010": {
        "gen": 4,
        "period": 1266,
        "data": {
          "available_products": [
            {
              "available_count": 1,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 1600,
                      "cost_type": 1001,
                      "origin_price": 1600
                    }
                  ]
                }
              ],
              "identified": true,
              "identified_items": [
                {
                  "basic": {
                    "count": 1,
                    "guid": "100000049",
                    "type_id": 183012000
                  },
                  "data": {
                    "affix_data": [
                      {
                        "affix_type": 10,
                        "attribute_random": {
                          "1105": {
                            "random_data": [
                              {
                                "affix_pool_value": true,
                                "group_index": 1,
                                "random_level": 7545
                              }
                            ]
                          },
                          "2002": {
                            "random_data": [
                              {
                                "affix_pool_value": true,
                                "group_index": 1,
                                "random_level": 8844
                              }
                            ]
                          }
                        },
                        "location": 1
                      }
                    ]
                  }
                }
              ],
              "last_auto_refresh_reset_time": "1792713600",
              "next_auto_refresh_reset_time": "1792800000",
              "product_id": 751221,
              "total_available_count": 1
            }
          ],
          "last_auto_refresh_reset_time": "1792713600",
          "market_tab_id": 1010,
          "next_auto_refresh_reset_time": "1792800000"
        }
      },
      "1011": {
        "gen": 4,
        "period": 1266,
        "data": {
          "available_products": [
            {
              "available_count": 1,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 1440,
                      "cost_type": 1001,
                      "origin_price": 1440
                    }
                  ]
                }
              ],
              "identified": true,
              "identified_items": [
                {
                  "basic": {
                    "count": 1,
                    "guid": "100000059",
                    "type_id": 185012002
                  },
                  "data": {
                    "affix_data": [
                      {
                        "affix_type": 10,
                        "attribute_random": {
                          "1003": {
                            "random_data": [
                              {
                                "affix_pool_value": true,
                                "group_index": 1,
                                "random_level": 3717
                              }
                            ]
                          },
                          "2008": {
                            "random_data": [
                              {
                                "affix_pool_value": true,
                                "group_index": 1,
                                "random_level": 620
                              }
                            ]
                          }
                        },
                        "location": 1
                      }
                    ]
                  }
                }
              ],
              "last_auto_refresh_reset_time": "1792713600",
              "next_auto_refresh_reset_time": "1792800000",
              "product_id": 751321,
              "total_available_count": 1
            }
          ],
          "last_auto_refresh_reset_time": "1792713600",
          "market_tab_id": 1011,
          "next_auto_refresh_reset_time": "1792800000"
        }
      },
      "1014": {
        "gen": 4,
        "period": 1266,
        "data": {
          "available_products": [
            {
              "available_count": 999,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 1600,
                      "cost_type": 1001,
                      "origin_price": 1600
                    }
                  ]
                }
              ],
              "last_auto_refresh_reset_time": "1792713600",
              "next_auto_refresh_reset_time": "1792800000",
              "product_id": 751419,
              "total_available_count": 999
            }
          ],
          "last_auto_refresh_reset_time": "1792713600",
          "market_tab_id": 1014,
          "next_auto_refresh_reset_time": "1792800000"
        }
      },
      "1015": {
        "gen": 4,
        "period": 1266,
        "data": {
          "available_products": [
            {
              "available_count": 999,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 10800,
                      "cost_type": 1001,
                      "origin_price": 10800
                    }
                  ]
                }
              ],
              "last_auto_refresh_reset_time": "1792713600",
              "next_auto_refresh_reset_time": "1792800000",
              "product_id": 751522,
              "total_available_count": 999
            }
          ],
          "last_auto_refresh_reset_time": "1792713600",
          "market_tab_id": 1015,
          "next_auto_refresh_reset_time": "1792800000"
        }
      },
      "1016": {
        "gen": 4,
        "period": 1266,
        "data": {
          "available_products": [
            {
              "available_count": 100,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 500,
                      "cost_type": 1001,
                      "origin_price": 500
                    }
                  ]
                }
              ],
              "last_auto_refresh_reset_time": "1792713600",
              "next_auto_refresh_reset_time": "1792800000",
              "product_id": 705001,
              "total_available_count": 100
            }
          ],
          "last_auto_refresh_reset_time": "1792713600",
          "market_tab_id": 1016,
          "next_auto_refresh_reset_time": "1792800000"
        }
      },
      "1017": {
        "gen": 4,
        "period": 1266,
        "data": {
          "available_products": [
            {
              "available_count": 5,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 20000,
                      "cost_type": 1001,
                      "origin_price": 20000
                    }
                  ]
                }
              ],
              "last_auto_refresh_reset_time": "1792713600",
              "next_auto_refresh_reset_time": "1792800000",
              "product_id": 706001,
              "total_available_count": 5
            }
          ],
          "last_auto_refresh_reset_time": "1792713600",
          "market_tab_id": 1017,
          "next_auto_refresh_reset_time": "1792800000"
        }
      },
      "1019": {
        "gen": 4,
        "period": 30396,
        "data": {
          "available_products": [
            {
              "available_count": 999,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 7500,
                      "cost_type": 1001,
                      "origin_price": 7500
                    }
                  ]
                }
              ],
              "last_auto_refresh_reset_time": "1792756800",
              "next_auto_refresh_reset_time": "1792760400",
              "product_id": 705013,
              "total_available_count": 999
            }
          ],
          "last_auto_refresh_reset_time": "1792756800",
          "market_tab_id": 1019,
          "next_auto_refresh_reset_time": "1792760400"
        }
      },
      "2000": {
        "gen": 4,
        "period": 0,
        "data": {
          "available_products": [
            {
              "available_count": 999,
              "cost_options": [
                {
                  "cost_items": [
                    {
                      "cost_price": 30000,
                      "cost_type": 1001,
                      "origin_price": 30000
                    }
                  ]
                }
              ],
              "last_auto_refresh_reset_time": "0",
              "next_auto_refresh_reset_time": "0",
              "product_id": 704016,
              "total_available_count": 999
            }
          ],
          "last_auto_refresh_reset_time": "0",
          "market_tab_id": 2000,
          "next_auto_refresh_reset_time": "0"
        }
      }
    },
    "sold": [
      {
        "item_data": {
          "basic": {
            "count": 1,
            "guid": "100000007",
            "type_id": 184050000
          },
          "data": {
            "affix_data": [
              {
                "affix_type": "EN_BATTLE_AFFIX_TYPE_MAJOR",
                "attribute_random": {
                  "1103": {
                    "random_data": [
                      {
                        "affix_pool_value": true,
                        "group_index": 1,
                        "random_level": 5411
                      }
                    ]
                  },
                  "2002": {
                    "random_data": [
                      {
                        "affix_pool_value": true,
                        "group_index": 1,
                        "random_level": 8678
                      }
                    ]
                  }
                },
                "location": "EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_EQUIP"
              }
            ]
          }
        },
        "last_sold_timepoint": "1792755996",
        "sold_version": "15"
      }
    ],
    "sold_version": 24
  },
  "affix_gen": 2,
  "quest": {
    "act": 16518,
    "join": 1792755843,
    "quests": {
      "1601000": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843
      },
      "1651809": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843
      },
      "1651812": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843
      },
      "1651814": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843
      },
      "1651818": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651819": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651820": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651821": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651822": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651823": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651824": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651825": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651826": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      },
      "1651827": {
        "status": 1,
        "values": [
          0
        ],
        "created": 1792755843,
        "period": 125
      }
    },
    "daily": [
      {
        "quest": 1651809,
        "idx": 1
      }
    ],
    "daily_idx": 3,
    "daily_day": 20749,
    "daily_at": 1792755843,
    "event_id": 1,
    "dispatched": 3
  },
  "heirloom": {
    "unlocked": [
      210140220
    ],
    "gems": [
      230192000
    ],
    "inlaid": {
      "21014020": [
        {
          "slot": 1,
          "gem": 240192000,
          "loc": "EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_EQUIP"
        }
      ]
    }
  },
  "raid_done": "6adb48b4bf76-11"
}
```
</details>

---
## 📄 Structure of the `data.json` file

| Field path (JSON / Rust) | Data type | Example value (what it should be) |
| :--- | :--- | :--- |
| `basic.type_id` | `int` | `400002` |
| `basic.meta.race` | `str` | `"EN_ROLE_RACE_HUMAN"` |
| `basic.meta.career` | `str` | `"EN_ROLE_CAREER_FIGHTER"` |
| `basic.meta.gender` | `str` | `"EN_ROLE_GENDER_FEMALE"` |
| `basic.meta.force` | `str` | `"EN_FORCE_TYPE_ADVENTURER"` |
| `basic.role_guid` | `str` | `"1000001"` |
| `basic.role_name` | `str` | `"1111111"` |
| `basic.create_index` | `str` | `"1"` |
| `basic.create_timepoint` | `str` | `"1792755892"` |
| `last_use_timepoint` | `str` | `"1792755892"` |
| `deploy_role.basic.type_id` | `int` | `400002` |
| `deploy_role.basic.meta.race` | `str` | `"EN_ROLE_RACE_HUMAN"` |
| `deploy_role.basic.meta.career` | `str` | `"EN_ROLE_CAREER_FIGHTER"` |
| `deploy_role.basic.meta.gender` | `str` | `"EN_ROLE_GENDER_FEMALE"` |
| `deploy_role.basic.meta.force` | `str` | `"EN_FORCE_TYPE_ADVENTURER"` |
| `deploy_role.basic.role_guid` | `str` | `"1000001"` |
| `deploy_role.basic.role_name` | `str` | `"1111111"` |
| `deploy_role.basic.create_index` | `str` | `"1"` |
| `deploy_role.basic.create_timepoint` | `str` | `"1792755892"` |
| `deploy_role.backpack.items[].basic.type_id` | `int` | `181126001` |
| `deploy_role.backpack.items[].basic.count` | `int` | `1` |
| `deploy_role.backpack.items[].basic.guid` | `str` | `"100000078"` |
| `deploy_role.backpack.items[].basic.position.type` | `str` | `"EN_ITEM_STORAGE_TYPE_BACKPACK"` |
| `deploy_role.backpack.items[].basic.position.guid` | `str` | `"1000001"` |
| `deploy_role.backpack.items[].basic.position.y` | `int` | `5` |
| `deploy_role.backpack.items[].data.affix_data[].location` | `str` | `"EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_HOLD"` |
| `deploy_role.backpack.items[].data.affix_data[].affix_type` | `str` | `"EN_BATTLE_AFFIX_TYPE_LEGENDARY"` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1101.random_data[].random_level` | `int` | `6052` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1002.random_data[].random_level` | `int` | `2243` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1002.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1002.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1021.random_data[].random_level` | `int` | `989` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1021.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1021.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6211.random_data[].random_level` | `int` | `6956` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6211.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6211.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6214.random_data[].random_level` | `int` | `6415` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6214.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6214.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1114.random_data[].random_level` | `int` | `7330` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1114.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1114.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181046000.random_data[].random_level` | `int` | `7265` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181046000.random_data[].legendary_affix_id` | `int` | `181046000` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1003.random_data[].random_level` | `int` | `174` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1003.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1003.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.2002.random_data[].random_level` | `int` | `7224` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.2002.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.2002.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6213.random_data[].random_level` | `int` | `1798` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6213.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6213.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181125001.random_data[].random_level` | `int` | `2024` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181125001.random_data[].legendary_affix_id` | `int` | `181125001` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1111.random_data[].random_level` | `int` | `6777` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1111.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1111.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181125000.random_data[].random_level` | `int` | `4589` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181125000.random_data[].legendary_affix_id` | `int` | `181125000` |
| `deploy_role.backpack.items[].basic.position.x` | `int` | `4` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1105.random_data[].random_level` | `int` | `2928` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1105.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1105.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1102.random_data[].random_level` | `int` | `6746` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1102.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1102.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6210.random_data[].random_level` | `int` | `2553` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6210.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.6210.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181116001.random_data[].random_level` | `int` | `9963` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181116001.random_data[].legendary_affix_id` | `int` | `181116001` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.4001.random_data[].random_level` | `int` | `1068` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.4001.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.4001.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181115001.random_data[].random_level` | `int` | `357` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181115001.random_data[].legendary_affix_id` | `int` | `181115001` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1104.random_data[].random_level` | `int` | `4591` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1104.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1104.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181015000.random_data[].random_level` | `int` | `1318` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181015000.random_data[].legendary_affix_id` | `int` | `181015000` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.2007.random_data[].random_level` | `int` | `4663` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.2007.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.2007.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181036001.random_data[].random_level` | `int` | `3981` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181036001.random_data[].legendary_affix_id` | `int` | `181036001` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1001.random_data[].random_level` | `int` | `5346` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1001.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1001.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1103.random_data[].random_level` | `int` | `9081` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1103.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1103.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1107.random_data[].random_level` | `int` | `7532` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1107.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1107.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181026001.random_data[].random_level` | `int` | `4800` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181026001.random_data[].legendary_affix_id` | `int` | `181026001` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1117.random_data[].random_level` | `int` | `5019` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1117.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.backpack.items[].data.affix_data[].attribute_random.1117.random_data[].group_index` | `int` | `1` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181126001.random_data[].random_level` | `int` | `2470` |
| `deploy_role.backpack.items[].data.affix_data[].ability_set_random.181126001.random_data[].legendary_affix_id` | `int` | `181126001` |
| `deploy_role.backpack.row_size` | `int` | `7` |
| `deploy_role.backpack.column_size` | `int` | `10` |
| `deploy_role.wearing.items[].basic.type_id` | `int` | `100012` |
| `deploy_role.wearing.items[].basic.count` | `int` | `3` |
| `deploy_role.wearing.items[].basic.guid` | `str` | `"100000008"` |
| `deploy_role.wearing.items[].basic.position.type` | `str` | `"EN_ITEM_STORAGE_TYPE_ROLE_WEARING"` |
| `deploy_role.wearing.items[].basic.position.guid` | `str` | `"1000001"` |
| `deploy_role.wearing.items[].basic.position.x` | `int` | `51` |
| `deploy_role.wearing.items[].data.affix_data[].location` | `str` | `"EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_EQUIP"` |
| `deploy_role.wearing.items[].data.affix_data[].affix_type` | `str` | `"EN_BATTLE_AFFIX_TYPE_LEGENDARY"` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1101.random_data[].random_level` | `int` | `726` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1001.random_data[].random_level` | `int` | `811` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1001.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1001.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1021.random_data[].random_level` | `int` | `4422` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1021.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1021.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6212.random_data[].random_level` | `int` | `9775` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6212.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6212.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6210.random_data[].random_level` | `int` | `9168` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6210.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6210.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1119.random_data[].random_level` | `int` | `3520` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1119.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1119.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.181025000.random_data[].random_level` | `int` | `694` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.181025000.random_data[].legendary_affix_id` | `int` | `181025000` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1002.random_data[].random_level` | `int` | `3750` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1002.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1002.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1104.random_data[].random_level` | `int` | `7004` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1104.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1104.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1103.random_data[].random_level` | `int` | `5472` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1103.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1103.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6214.random_data[].random_level` | `int` | `505` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6214.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.6214.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.181015001.random_data[].random_level` | `int` | `7008` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.181015001.random_data[].legendary_affix_id` | `int` | `181015001` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1105.random_data[].random_level` | `int` | `1655` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1105.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1105.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.181016000.random_data[].random_level` | `int` | `5526` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.181016000.random_data[].legendary_affix_id` | `int` | `181016000` |
| `deploy_role.wearing.items[].data.gem_data.slot_data[].slot_id` | `int` | `3` |
| `deploy_role.wearing.items[].data.gem_data.slot_data[].gem_id` | `int` | `240184000` |
| `deploy_role.wearing.items[].data.gem_data.slot_data[].affix_location` | `str` | `"EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_EQUIP"` |
| `deploy_role.wearing.items[].create_info.create_timestamp` | `str` | `"2026-10-23T12:06:08Z"` |
| `deploy_role.wearing.items[].create_info.heirloom.owner.user_id` | `str` | `"10001"` |
| `deploy_role.wearing.items[].create_info.heirloom.owner.zone_id` | `int` | `2200` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2002.random_data[].random_level` | `int` | `5934` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2002.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2002.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1006.random_data[].random_level` | `int` | `8122` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1006.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1006.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2011.random_data[].random_level` | `int` | `6532` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2011.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2011.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2009.random_data[].random_level` | `int` | `4862` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2009.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2009.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.189010000.random_data[].random_level` | `int` | `2486` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.189010000.random_data[].legendary_affix_id` | `int` | `189010000` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1005.random_data[].random_level` | `int` | `5224` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1005.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1005.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1115.random_data[].random_level` | `int` | `7631` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1115.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1115.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1106.random_data[].random_level` | `int` | `2700` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1106.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1106.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].create_info.temporary_item_guid` | `str` | `"6ac5926f-2d2a-42c8-89bb-5e2d40e98a12"` |
| `deploy_role.wearing.items[].create_info.loot` | `Object (empty)` | `{}` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2008.random_data[].random_level` | `int` | `7497` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2008.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.2008.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.185015001.random_data[].random_level` | `int` | `2665` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.185015001.random_data[].legendary_affix_id` | `int` | `185015001` |
| `deploy_role.wearing.items[].data.affix_data[].gear_set_random.11005.random_data[].random_level` | `int` | `8907` |
| `deploy_role.wearing.items[].data.affix_data[].gear_set_random.11005.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1107.random_data[].random_level` | `int` | `2059` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1107.random_data[].affix_pool_value` | `bool` | `true` |
| `deploy_role.wearing.items[].data.affix_data[].attribute_random.1107.random_data[].group_index` | `int` | `1` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.185015002.random_data[].random_level` | `int` | `8433` |
| `deploy_role.wearing.items[].data.affix_data[].ability_set_random.185015002.random_data[].legendary_affix_id` | `int` | `185015002` |
| `deploy_role.dress_up.current_face_pinching.face_type` | `int` | `101` |
| `deploy_role.dress_up.current_face_pinching.hair_type` | `int` | `101` |
| `deploy_role.dress_up.current_face_pinching.beard_type` | `int` | `1` |
| `deploy_role.dress_up.current_face_pinching.skin_type` | `int` | `1` |
| `deploy_role.dress_up.current_face_pinching.hair_color` | `int` | `1` |
| `deploy_role.dress_up.current_face_pinching.beard_color` | `int` | `1` |
| `deploy_role.dress_up.current_face_pinching.tatoo_type` | `int` | `1` |
| `deploy_role.dress_up.current_face_pinching.tatoo_color` | `int` | `1` |
| `deploy_role.dress_up.current_face_pinching.eye_color` | `int` | `8` |
| `deploy_role.dress_up.current_face_pinching.scar_type` | `int` | `1` |
| `deploy_role.dress_up.current_face_pinching.makeup_type` | `int` | `1` |
| `deploy_role.dress_up.current_face_pinching.makeup_color` | `int` | `1` |
| `deploy_role.dress_up.dress_facade_info` | `Object (empty)` | `{}` |
| `deploy_role.role_level` | `int` | `2` |
| `deploy_role.role_affix.attribute.1001` | `int` | `30` |
| `deploy_role.role_affix.attribute.1002` | `int` | `6` |
| `deploy_role.role_affix.attribute.1003` | `int` | `16` |
| `deploy_role.role_affix.attribute.1004` | `int` | `6` |
| `deploy_role.role_affix.attribute.1005` | `int` | `8` |
| `deploy_role.role_affix.attribute.1006` | `int` | `6` |
| `deploy_role.role_affix.attribute.1103` | `int` | `0` |
| `deploy_role.role_affix.attribute.1104` | `float` | `0.2` |
| `deploy_role.role_affix.attribute.1105` | `int` | `1` |
| `deploy_role.role_affix.attribute.1106` | `float` | `0.1` |
| `deploy_role.role_affix.attribute.1107` | `int` | `0` |
| `deploy_role.role_affix.attribute.2001` | `int` | `558` |
| `deploy_role.role_affix.attribute.2007` | `float` | `0.1` |
| `deploy_role.role_affix.attribute.2009` | `float` | `0.05` |
| `deploy_role.role_affix.attribute.2010` | `float` | `0.05` |
| `deploy_role.role_affix.attribute.2011` | `float` | `0.05` |
| `deploy_role.role_affix.attribute.2012` | `float` | `0.05` |
| `deploy_role.role_affix.attribute.2013` | `float` | `0.05` |
| `deploy_role.role_affix.attribute.3001` | `int` | `330` |
| `deploy_role.role_skill.role_active_skill[].key_position` | `str` | `"EN_ROLE_SKILL_POSITION_E"` |
| `deploy_role.role_skill.role_active_skill[].skill_id` | `int` | `107002` |
| `deploy_role.exp` | `int` | `15` |
| `deploy_role.small_backpack` | `Object (empty)` | `{}` |
| `deploy_role.history_role_max_level` | `int` | `2` |

<details><summary><b>Show a compact JSON template (1 element per array)</b></summary>

```json
{
  "basic": {
    "type_id": 400002,
    "meta": {
      "race": "EN_ROLE_RACE_HUMAN",
      "career": "EN_ROLE_CAREER_FIGHTER",
      "gender": "EN_ROLE_GENDER_FEMALE",
      "force": "EN_FORCE_TYPE_ADVENTURER"
    },
    "role_guid": "1000001",
    "role_name": "1111111",
    "create_index": "1",
    "create_timepoint": "1792755892"
  },
  "last_use_timepoint": "1792755892",
  "deploy_role": {
    "basic": {
      "type_id": 400002,
      "meta": {
        "race": "EN_ROLE_RACE_HUMAN",
        "career": "EN_ROLE_CAREER_FIGHTER",
        "gender": "EN_ROLE_GENDER_FEMALE",
        "force": "EN_FORCE_TYPE_ADVENTURER"
      },
      "role_guid": "1000001",
      "role_name": "1111111",
      "create_index": "1",
      "create_timepoint": "1792755892"
    },
    "backpack": {
      "items": [
        {
          "basic": {
            "type_id": 181046000,
            "count": 1,
            "guid": "100000090",
            "position": {
              "type": "EN_ITEM_STORAGE_TYPE_BACKPACK",
              "guid": "1000001",
              "y": 8
            }
          },
          "data": {
            "affix_data": [
              {
                "location": "EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_HOLD",
                "affix_type": "EN_BATTLE_AFFIX_TYPE_BASIC",
                "attribute_random": {
                  "1101": {
                    "random_data": [
                      {
                        "random_level": 4405
                      }
                    ]
                  }
                }
              }
            ]
          }
        }
      ],
      "row_size": 7,
      "column_size": 10
    },
    "wearing": {
      "items": [
        {
          "basic": {
            "type_id": 181025000,
            "count": 1,
            "guid": "100000098",
            "position": {
              "type": "EN_ITEM_STORAGE_TYPE_ROLE_WEARING",
              "guid": "1000001",
              "x": 1
            }
          },
          "data": {
            "affix_data": [
              {
                "location": "EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_HOLD",
                "affix_type": "EN_BATTLE_AFFIX_TYPE_BASIC",
                "attribute_random": {
                  "1101": {
                    "random_data": [
                      {
                        "random_level": 9455
                      }
                    ]
                  }
                }
              }
            ]
          }
        }
      ]
    },
    "dress_up": {
      "current_face_pinching": {
        "face_type": 101,
        "hair_type": 101,
        "beard_type": 1,
        "skin_type": 1,
        "hair_color": 1,
        "beard_color": 1,
        "tatoo_type": 1,
        "tatoo_color": 1,
        "eye_color": 8,
        "scar_type": 1,
        "makeup_type": 1,
        "makeup_color": 1
      },
      "dress_facade_info": {}
    },
    "role_level": 2,
    "role_affix": {
      "attribute": {
        "1001": 30,
        "1002": 6,
        "1003": 16,
        "1004": 6,
        "1005": 8,
        "1006": 6,
        "1103": 0,
        "1104": 0.2,
        "1105": 1,
        "1106": 0.1,
        "1107": 0,
        "2001": 558,
        "2007": 0.1,
        "2009": 0.05,
        "2010": 0.05,
        "2011": 0.05,
        "2012": 0.05,
        "2013": 0.05,
        "3001": 330
      }
    },
    "role_skill": {
      "role_active_skill": [
        {
          "key_position": "EN_ROLE_SKILL_POSITION_Q",
          "skill_id": 107001
        }
      ]
    },
    "exp": 15,
    "small_backpack": {},
    "history_role_max_level": 2
  }
}
```
</details>

---
