#!/bin/bash
# 14.11: backend b976aae (= 9374452 "Tipsy Alchemist" + 4df69c1 instance shift + c2aa021 raidrestore + swordmaster swords),
# ExcelConfig/affix_pool.bytes, ru.balance 2026.1114.530 (pak d8d44788). Deployed "softly" (maintenance first), with the owner's go-ahead.
# Phase wait: maintenance on and wait for battles to end. Phase swap: replacement (after the pak on Robot).
set -e
cd /opt/dungeonborne
T=stage_20261114
exec >> $T/swap.log 2>&1
case "$1" in
wait)
  echo "=== $(date -u) wait"
  [ -e maintenance.txt ] || ./maintenance.sh on "Короткие технические работы: обновление сервера после текущих боёв.
[en]
Short maintenance: server update after the running battles."
  ./maintenance.sh wait 50 || true
  ./maintenance.sh status || true
  echo "=== $(date -u) wait done"
  ;;
swap)
  echo "=== $(date -u) swap"
  cat $T/b97.gz.* > $T/dbserver-rs-b976aae.gz
  gunzip -c $T/dbserver-rs-b976aae.gz > $T/dbserver-rs-b976aae
  sha=$(sha256sum $T/dbserver-rs-b976aae | cut -c1-64)
  echo "sha $sha"
  [ "$sha" = "6a58060e5d21ccd45135634c1c0e5e2a04ae93a03fe43ca70917e9bf42d9765b" ] || { echo "sha mismatch - stop"; exit 1; }
  [ "$(sha256sum $T/affix_pool.bytes | cut -c1-16)" = "ebc2ddfc2e6a2786" ] || { echo "affix_pool sha - stop"; exit 1; }
  systemctl stop dbserver
  B=backup_20261114
  mkdir -p $B
  cp -a db_rocks $B/db_rocks
  cp -p dbserver-rs $B/dbserver-rs.2250bea
  cp -p ExcelConfig/affix_pool.bytes $B/affix_pool.bytes
  cp -p mods/manifest_mods.json $B/manifest_mods.json
  cp -p server.toml $B/server.toml
  install -o dungeonborne -g dungeonborne -m 755 $T/dbserver-rs-b976aae dbserver-rs
  install -o dungeonborne -g dungeonborne -m 644 $T/affix_pool.bytes ExcelConfig/affix_pool.bytes
  install -o dungeonborne -g dungeonborne -m 644 $T/ru.balance-2026.1114.530.zip mods/ru.balance-2026.1114.530.zip
  python3 - <<'PY'
import json
p = "mods/manifest_mods.json"
d = json.load(open(p))
e = json.load(open("stage_20261114/ru.balance.entry.json"))
n = 0
for m in d:
    if m["id"] == "ru.balance":
        m.update({k: e[k] for k in ("version", "sha256", "size", "url")}); n += 1
assert n == 1
json.dump(d, open(p + ".new", "w"), indent=1, ensure_ascii=False)
print("manifest ru.balance ->", e["version"])
PY
  chown --reference=mods/manifest_mods.json mods/manifest_mods.json.new
  chmod --reference=mods/manifest_mods.json mods/manifest_mods.json.new
  mv mods/manifest_mods.json.new mods/manifest_mods.json
  mkdir -p admin_cmd restore && chown dungeonborne:dungeonborne admin_cmd restore && chmod 700 admin_cmd restore
  cp -p "$T/gameover-<steam_id>-3-082640.bin" restore/ && chown dungeonborne:dungeonborne restore/*.bin
  systemctl start dbserver
  sleep 10
  systemctl is-active dbserver
  echo "=== $(date -u) swap done (b976aae), maintenance is still on"
  ;;
esac
