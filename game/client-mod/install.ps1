# Dungeonborne mod installation (administrator rights required)
param([string]$GameRoot = '')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

# Administrator rights check
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    Write-Host "Запрашиваем права администратора..." -ForegroundColor Yellow
    $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"")
    if ($GameRoot) { $a += @('-GameRoot', "`"$GameRoot`"") }
    Start-Process powershell.exe -Verb RunAs -ArgumentList $a
    exit
}

$domains = @('sg.intlgame.com', 'intlgame.com', 'openapi.smartlink.intlgame.com', 'tdm.deltaverse-intl.com',
    'f.tgf123.com', 'sg.api.tgf123.com', 'tgf123.com',
    'dir.228918463-2-1.dmpplat.com', 'tconnd-7101-702.dmpplat.com', 't01.tgf123.com')

$begin = '# Dungeonborne Custom Server begin'
$end = '# Dungeonborne Custom Server end'
$ansi = [Text.Encoding]::GetEncoding(28591)

$files = Split-Path -Parent $PSCommandPath
$hostsPath = Join-Path $env:SystemRoot 'System32\drivers\etc\hosts'

function Fail($msg) { Write-Host "[Ошибка] $msg" -ForegroundColor Red; Read-Host 'Нажмите Enter для выхода'; exit 1 }
function Ok($msg) { Write-Host "[Успешно] $msg" -ForegroundColor Green }
function IsGame($dir) { $dir -and (Test-Path (Join-Path $dir 'Dungeonborne\Binaries\Win64\Dungeonborne.exe')) }

Write-Host '=== Установка Dungeonborne Custom Server Mod ===' -ForegroundColor Cyan

# Find the game folder
$root = $GameRoot
while (-not (IsGame $root)) {
    $root = (Read-Host 'Введите путь к папке с игрой (Например: <библиотека Steam>\steamapps\common\Dungeonborne)').Trim().Trim('"').TrimEnd('\')
    if (-not $root) { Fail 'Путь не указан.' }
}

$win64 = Join-Path $root 'Dungeonborne\Binaries\Win64'
$paks = Join-Path $root 'Dungeonborne\Content\Paks'

if (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) { Fail 'Пожалуйста, закройте игру перед установкой.' }

# 1. Copy the Pak files
try {
    $pakFolder = Join-Path $files 'Paks'
    if (Test-Path $pakFolder) {
        foreach ($p in Get-ChildItem $pakFolder -Filter '*.pak') {
            Copy-Item -Force $p.FullName (Join-Path $paks $p.Name)
        }
        Ok 'Моды (.pak) скопированы в папку игры.'
    } else {
        Write-Host "[Предупреждение] Папка 'Paks' не найдена рядом со скриптом. Пропуск." -ForegroundColor Yellow
    }
} catch {
    Fail "Ошибка копирования .pak файлов: $($_.Exception.Message)"
}

# 2. Create steam_appid.txt
$appid = Join-Path $win64 'steam_appid.txt'
if (-not (Test-Path $appid)) {
    [IO.File]::WriteAllText($appid, '2448970')
}
Ok 'Файл steam_appid.txt создан.'

# 3. Install the cacert.pem certificate
$cacert = Join-Path $win64 'cacert.pem'
$caPath = Join-Path $files 'cacert.pem'
if (Test-Path $caPath) {
    $ca = [IO.File]::ReadAllText($caPath).Trim()
    $body = ($ca -replace '-----[A-Z ]+-----', '' -replace '\s', '')
    
    if (Test-Path $cacert) {
        $cur = [IO.File]::ReadAllText($cacert)
        if (-not ($cur -replace '\s', '').Contains($body)) {
            $bak = "$cacert.backup"
            if (-not (Test-Path $bak)) { Copy-Item $cacert $bak }
            [IO.File]::AppendAllText($cacert, "`n# Dungeonborne Custom Server CA`n$ca`n")
        }
    } else {
        Copy-Item $caPath $cacert
    }
    Ok 'Сертификат cacert.pem установлен.'
} else {
    Write-Host "[Предупреждение] Файл 'cacert.pem' не найден рядом со скриптом. Пропуск." -ForegroundColor Yellow
}

# 4. Edit the hosts file
try {
    $bak = "$hostsPath.backup"
    if (-not (Test-Path $bak)) { Copy-Item $hostsPath $bak }
    $raw = [IO.File]::ReadAllText($hostsPath, $ansi)
    $kept = ($raw -replace "(?s)\r?\n?$([regex]::Escape($begin)).*?$([regex]::Escape($end))\r?\n?", "`r`n").TrimEnd()
    $block = @($begin) + ($domains | ForEach-Object { "127.0.0.1 $_" }) + @($end)
    [IO.File]::WriteAllText($hostsPath, $kept + "`r`n" + ($block -join "`r`n") + "`r`n", $ansi)
    ipconfig /flushdns | Out-Null
    Ok "Файл hosts успешно обновлен (домены перенаправлены на 127.0.0.1)."
} catch {
    Fail "Ошибка изменения файла hosts (возможно блокирует антивирус): $($_.Exception.Message)"
}

Write-Host ''
Write-Host 'Установка успешно завершена! Теперь вы можете запустить dbserver-rs.exe и зайти в игру.' -ForegroundColor Cyan