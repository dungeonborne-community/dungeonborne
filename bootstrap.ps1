<#
.SYNOPSIS
  Dungeonborne DevKit bootstrap: an empty folder -> workspace + home + vault -> DevKit.

.DESCRIPTION
  Steps (docs/WORKSPACE_PLAN.md, section 13), in this order:
    machine    checks: 64-bit Windows, PowerShell 5.1+, free space
    rustup     rustup-init (sha256 pinned) into home\toolchains, user PATH untouched
    git        git from PATH, otherwise PortableGit (sha256 pinned, with Git Bash) into home\tools\git
    clone      git clone of the repository into workspace (with -Components: partial, the base only)
    toolchain  the Rust version from rust-toolchain.toml
    linker     Visual Studio Build Tools if present, otherwise cargo-xwin (Microsoft CRT + Windows SDK)
    tools      programs from tools.lock (trunk, tauri-cli, tailwindcss; sha256 pinned) into home\tools
    devkit     ws and DevKit: a signed dist if there is one, otherwise built here
    components with -Components: the chosen components on disk (ws components set; docs\COMPONENTS_PLAN.md)

  - Everything goes under -Root (workspace, home, vault). No admin rights, nothing installed system-wide,
    the user PATH and environment are not changed (rustup runs with --no-modify-path).
  - Every download is pinned by sha256 in config\bootstrap.manifest.toml or config\tools.lock; a mismatch stops.
  - Without -Apply the script only prints the plan and changes nothing. -Status shows which steps are done.
  - Running it again is safe: finished steps are skipped. A failed step can be repeated alone with -Step.

  For AI agents (AGENTS.md): read this script and config\bootstrap.manifest.toml first and give the human a short
  conclusion (what it downloads and where, all pinned by sha256; what it does not touch), show -Status and the plan,
  then run the steps with -Apply only after their "yes" - all at once or one -Step at a time.
  If your tools refuse to run this script (an AI tool's safety mode, e.g. Claude Code "auto"), do not look for other
  ways to run it: give the human your conclusion and the exact commands (plan, then -Apply) and ask them to run it
  themselves; when they say it finished, check with -Status (read-only) and continue.

.PARAMETER Root
  Folder for workspace, home and vault (empty or an earlier bootstrap). Default: the parent of this script's
  folder when the script lies inside a workspace.
.PARAMETER Step
  all (default) or one step: machine, rustup, git, clone, toolchain, linker, tools, devkit. A step whose earlier
  steps are not done stops with the step to run first.
.PARAMETER Tool
  With -Step tools: only these tools from tools.lock, comma separated (repeat one failed download).
.PARAMETER Status
  Print each step as done / todo and the next step to run. Changes nothing (it may build xtask to check the tools).
.PARAMETER Repo
  git URL or local path of the repository. Default: [repo] url from the manifest.
.PARAMETER Ref
  Branch, tag or commit to check out. Default: [repo] ref. A full 40-character commit hash is verified after checkout.
.PARAMETER Build
  prebuilt (default): signed dist for ws and DevKit when available, else built here.
  source: build ws and DevKit here.
  Third-party tools (trunk, tauri-cli, tailwindcss, LLVM...) are always their official releases, sha256 pinned in
  config\tools.lock: building them here adds nothing and breaks without Visual Studio (libdeflate-sys under clang-cl,
  19.11). To build them anyway: cargo xtask setup --only=tools --apply --from-source.
.PARAMETER Components
  Only some components on disk (config\components.toml): a preset (web, balance, modding, re, full) or component
  names, comma separated. The clone is partial (git --filter=blob:none --sparse): the base first, the rest in the
  step components; what they need comes with them. Later: cargo ws components list / add / remove / set.
  Without it: a full clone, everything on disk.
.PARAMETER Linker
  auto (default): Visual Studio Build Tools if found, otherwise cargo-xwin. msvc / xwin force one of them.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root X:\DBDev -Status            # what is done
  powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root X:\DBDev                    # plan only
  powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root X:\DBDev -Apply             # everything
  powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root X:\DBDev -Step clone -Apply # one step
  powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root X:\DBDev -Components web -Apply  # the site only
#>
[CmdletBinding()]
param(
    [string]$Root,
    [ValidateSet('all', 'machine', 'rustup', 'git', 'clone', 'toolchain', 'linker', 'tools', 'devkit', 'components')][string]$Step = 'all',
    [string]$Tool,
    [switch]$Status,
    [string]$Repo,
    [string]$Ref,
    [ValidateSet('prebuilt', 'source')][string]$Build = 'prebuilt',
    [ValidateSet('auto', 'msvc', 'xwin')][string]$Linker = 'auto',
    [string]$Components,
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Steps = @('machine', 'rustup', 'git', 'clone', 'toolchain', 'linker', 'tools', 'devkit', 'components')
$script:Current = ''
$script:Bound = @{} + $PSBoundParameters

function Say([string]$tag, [string]$msg) { Write-Host ('{0,-6} {1}' -f $tag, $msg) }
# Parameters to repeat in hints (a fork's -Repo, -Linker xwin ...): the same command, only another step.
function Get-Extra {
    $x = ''
    foreach ($k in 'Repo', 'Ref', 'Build', 'Linker', 'Components') {
        if ($script:Bound.ContainsKey($k)) { $x += " -$k $($script:Bound[$k])" }
    }
    $x
}
function Fail([string]$msg) {
    Write-Host "ERROR  $msg" -ForegroundColor Red
    if ($script:Current) {
        $t = if ($script:Current -eq 'tools' -and $Tool) { " -Tool $Tool" } else { '' }
        Write-Host "retry  this step alone: bootstrap.ps1 -Root $Root$(Get-Extra) -Step $($script:Current)$t -Apply"
    }
    exit 1
}

# --- manifest: flat [section] key = "value" (comments after # are ignored)
function Read-Manifest([string]$path) {
    $m = @{}; $sec = ''
    foreach ($line in Get-Content -LiteralPath $path) {
        $t = ($line -replace '\s+#.*$', '').Trim()
        if (-not $t -or $t.StartsWith('#')) { continue }
        if ($t -match '^\[(.+)\]$') { $sec = $matches[1]; $m[$sec] = @{}; continue }
        if ($t -match '^([\w-]+)\s*=\s*"(.*)"$') { $m[$sec][$matches[1]] = $matches[2] }
    }
    $m
}

function Get-Sha256([string]$file) { (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLower() }

# Download url -> file, check sha256 (an already downloaded file with the right hash is reused). Downloads go to
# <file>.part first, so a broken download never sits under the real name.
function Get-Pinned([string]$url, [string]$sha, [string]$file) {
    if ((Test-Path -LiteralPath $file) -and (Get-Sha256 $file) -eq $sha) { return }
    $part = "$file.part"
    & "$env:SystemRoot\System32\curl.exe" -fsSL --retry 5 --retry-all-errors --retry-delay 2 -o $part $url
    if ($LASTEXITCODE -ne 0) { Remove-Item -LiteralPath $part -Force -ErrorAction SilentlyContinue; Fail "download failed: $url (network? run the step again)" }
    $h = Get-Sha256 $part
    if ($h -ne $sha) { Remove-Item -LiteralPath $part -Force; Fail "$url : sha256 $h does not match the manifest - file deleted" }
    Move-Item -LiteralPath $part -Destination $file -Force
}

function Expand-Archive2([string]$archive, [string]$dir) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    & "$env:SystemRoot\System32\tar.exe" -xf $archive -C $dir
    if ($LASTEXITCODE -ne 0) { Fail "unpacking failed: $archive" }
}

# Native programs write progress to stderr; with redirected output (agents) Windows PowerShell would turn that into a
# terminating error under 'Stop' - so native calls run with 'Continue' and are judged by their exit code only.
function Invoke-Checked([string]$what, [scriptblock]$cmd) {
    $ErrorActionPreference = 'Continue'
    & $cmd
    if ($LASTEXITCODE -ne 0) { Fail "$what (exit code $LASTEXITCODE)" }
}

function In-Workspace([scriptblock]$body) {
    Push-Location $ws
    try { & $body } finally { Pop-Location }
}

# ------------------------------------------------------------------------------------------------ where
$here = $PSScriptRoot
$manifestPath = Join-Path $here 'config\bootstrap.manifest.toml'
if (-not (Test-Path -LiteralPath $manifestPath)) { Fail "config\bootstrap.manifest.toml must lie next to this script (keep the repository's folder layout)" }
$m = Read-Manifest $manifestPath
# Inside a workspace = a git clone with config\workspace.toml. A loose copy of the config folder next to the script
# (an archive sent to a newcomer) is not a workspace: the clone goes to <Root>\workspace (17.11, trial install).
$insideWorkspace = (Test-Path -LiteralPath (Join-Path $here 'config\workspace.toml')) -and (Test-Path -LiteralPath (Join-Path $here '.git'))
if (-not $Root) {
    if ($insideWorkspace) { $Root = Split-Path $here -Parent } else { Fail "-Root is required (an empty folder, for example X:\DBDev)" }
}
$Root = [IO.Path]::GetFullPath($Root)
$ws = if ($insideWorkspace -and ((Split-Path $here -Parent) -eq $Root)) { $here } else { Join-Path $Root 'workspace' }
$dbHome = Join-Path $Root 'home'
$vault = Join-Path $Root 'vault'
$tmp = Join-Path $dbHome 'cache\tmp'
if (-not $Repo) { $Repo = $m['repo']['url'] }
if (-not $Ref) { $Ref = $m['repo']['ref'] }
if ($Tool -and $Step -ne 'tools') { Fail '-Tool goes with -Step tools' }

# Environment of every step: everything inside home, nothing in the user profile.
$env:RUSTUP_HOME = Join-Path $dbHome 'toolchains\rustup'
$env:CARGO_HOME = Join-Path $dbHome 'toolchains\cargo'
$env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
$env:CARGO_TARGET_DIR = Join-Path $dbHome 'cache\target'
$rustup = Join-Path $env:CARGO_HOME 'bin\rustup.exe'
$portGit = Join-Path $dbHome 'tools\git'
if (-not (Get-Command git -ErrorAction SilentlyContinue) -and (Test-Path -LiteralPath "$portGit\cmd\git.exe")) { $env:PATH = "$portGit\cmd;" + $env:PATH }
$cargoConfig = Join-Path $env:CARGO_HOME 'config.toml'
$xwinMarker = '# dbdev-bootstrap: xwin'
if ((Test-Path -LiteralPath $cargoConfig) -and ((Get-Content -LiteralPath $cargoConfig -Raw) -match [regex]::Escape($xwinMarker))) { $env:DBDEV_XWIN = '1' }

# ------------------------------------------------------------------------------------------------ components
# -Components given and not "all"/"full": a partial clone (docs\COMPONENTS_PLAN.md)
function Test-Partial { [bool]$Components -and ($Components -notmatch '^\s*(all|full)\s*$') }

# Partial clone: the base only - top-level folders without members (docs, agents, config ...) and the members of the
# base component (config\components.toml). The step components puts the chosen ones next to it (ws components set).
function Set-BaseSparse {
    Invoke-Checked 'git sparse-checkout set config' { git -C $ws sparse-checkout set --cone config }
    $comp = Join-Path $ws 'config\components.toml'
    if (-not (Test-Path -LiteralPath $comp)) { Fail "-Components: this repository has no config\components.toml - run without -Components" }
    $members = @(); $in = $false
    foreach ($l in Get-Content -LiteralPath (Join-Path $ws 'config\workspace.toml')) {
        if ($l -match '^\s*members\s*=\s*\[') { $in = $true; continue }
        if ($in) {
            if ($l -match '^\s*\]') { break }
            if ($l -match '^\s*"([^"]+)"') { $members += $matches[1] }
        }
    }
    $base = @(); $isBase = $false; $list = @()
    foreach ($l in @(Get-Content -LiteralPath $comp) + '[[end]]') {
        if ($l -match '^\[\[') { if ($isBase) { $base = $list }; $isBase = $false; $list = @(); continue }
        if ($l -match '^\s*base\s*=\s*true') { $isBase = $true }
        if ($l -match '^\s*members\s*=\s*\[(.*)\]') { $list = @([regex]::Matches($matches[1], '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }) }
    }
    $top = & { $ErrorActionPreference = 'Continue'; git -C $ws ls-tree -d --name-only HEAD }
    $paths = @($top | Where-Object { $d = $_; -not ($members | Where-Object { $_ -eq $d -or $_.StartsWith("$d/") }) }) + $base
    Invoke-Checked 'git sparse-checkout set (base)' { git -C $ws sparse-checkout set --cone @paths }
    Say 'note' "partial clone, base only: $($paths -join ', ')"
}

# ------------------------------------------------------------------------------------------------ checks per step
function Get-Channel {
    $f = Join-Path $ws 'rust-toolchain.toml'
    if (-not (Test-Path -LiteralPath $f)) { return $null }
    $line = Get-Content -LiteralPath $f | Where-Object { $_ -match '^\s*channel\s*=\s*"(.+)"' } | Select-Object -First 1
    if ($line -match '"(.+)"') { $matches[1] } else { $null }
}
function Find-VS {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { return $null }
    & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
function Use-Xwin { ($Linker -eq 'xwin') -or ($Linker -eq 'auto' -and -not (Find-VS)) }
function Get-DevkitExe {
    foreach ($p in (Join-Path $dbHome 'tools\devkit\DevKit.exe'), (Join-Path $env:CARGO_TARGET_DIR 'release\DevKit.exe')) {
        if (Test-Path -LiteralPath $p) { return $p }
    }
    $null
}

# Returns @(done, detail).
function Test-Step([string]$s) {
    switch ($s) {
        'machine' {
            $drive = Get-PSDrive -Name $Root.Substring(0, 1)
            $free = [math]::Round($drive.Free / 1GB)
            return @(($free -ge 20 -and [Environment]::Is64BitOperatingSystem), "$free GB free on $($drive.Name): (about 20 GB needed)")
        }
        'rustup' { return @((Test-Path -LiteralPath $rustup), $rustup) }
        'git' {
            $g = Get-Command git -ErrorAction SilentlyContinue
            return @([bool]$g, $(if ($g) { $g.Source } else { "PortableGit -> $portGit" }))
        }
        'clone' {
            if (-not (Test-Path -LiteralPath (Join-Path $ws 'config\workspace.toml'))) { return @($false, "$Repo -> $ws") }
            # git writes to stderr when $ws is not a clone; with ErrorActionPreference Stop that would end the script
            $head = try { & { $ErrorActionPreference = 'Continue'; (git -C $ws log -1 --format='%h %s' 2>$null | Out-String).Trim() } } catch { '' }
            return @($true, "$ws ($head)")
        }
        'toolchain' {
            $ch = Get-Channel
            if (-not $ch) { return @($false, 'version from rust-toolchain.toml (after clone)') }
            $dir = Join-Path $env:RUSTUP_HOME "toolchains\$ch-x86_64-pc-windows-msvc"
            return @((Test-Path -LiteralPath $dir), "Rust $ch")
        }
        'linker' {
            $vs = Find-VS
            if ($Linker -ne 'xwin' -and $vs) { return @($true, "Visual Studio Build Tools ($vs)") }
            $crt = Join-Path $dbHome 'cache\xwin\xwin\crt\lib\x86_64'
            $ok = [bool]$env:DBDEV_XWIN -and (Test-Path -LiteralPath $crt)
            return @($ok, 'cargo-xwin: Microsoft CRT + Windows SDK into home\cache\xwin, about 1 GB')
        }
        'tools' {
            $t = Test-Step 'toolchain'
            if (-not $t[0]) { return @($false, 'tools.lock -> home\tools (after toolchain)') }
            $ok = In-Workspace {
                $ErrorActionPreference = 'Continue'
                & cargo xtask setup --only=tools --prebuilt *> $null
                $LASTEXITCODE -eq 0
            }
            return @($ok, 'tools.lock -> home\tools')
        }
        'devkit' {
            # Done only when both are installed in home\tools: a DevKit.exe left in the build cache does not count
            # (older installs built it there without copying ws and DevKit to home\tools).
            $dk = Join-Path $dbHome 'tools\devkit\DevKit.exe'
            $wsExe = Join-Path $dbHome 'tools\ws\ws.exe'
            $ok = (Test-Path -LiteralPath $dk) -and (Test-Path -LiteralPath $wsExe)
            return @($ok, $(if ($ok) { $dk } else { 'ws and DevKit -> home\tools (signed dist or built here)' }))
        }
        'components' {
            if (-not (Test-Partial)) { return @($true, 'everything on disk (no -Components)') }
            $lt = Join-Path $ws 'config\local.toml'
            $line = if (Test-Path -LiteralPath $lt) { Get-Content -LiteralPath $lt | Where-Object { $_ -match '^\s*installed\s*=' } | Select-Object -First 1 } else { $null }
            return @([bool]$line, $(if ($line) { ($line -replace '\s+#.*$', '').Trim() } else { "ws components set $Components" }))
        }
    }
}

# ------------------------------------------------------------------------------------------------ steps
function Step-Machine {
    if (-not [Environment]::Is64BitOperatingSystem) { Fail '64-bit Windows is required' }
    if ($PSVersionTable.PSVersion.Major -lt 5) { Fail 'Windows PowerShell 5.1 or newer is required' }
    $r = Test-Step 'machine'
    if ($r[0]) { Say 'ok' $r[1] } else { Say 'WARN' $r[1] }
    $admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if ($admin) { Say 'note' 'running as administrator is not needed' }
    if ($Apply) { foreach ($d in $Root, $vault, $dbHome, "$dbHome\toolchains", "$dbHome\tools", $tmp) { New-Item -ItemType Directory -Force -Path $d | Out-Null } }
}

function Step-Rustup {
    if (Test-Path -LiteralPath $rustup) {
        Say 'ok' "rustup: $rustup"
    } elseif (-not $Apply) {
        Say 'plan' "rustup-init (sha256 pinned) -> $env:CARGO_HOME, toolchains -> $env:RUSTUP_HOME (user PATH untouched)"
        return
    } else {
        New-Item -ItemType Directory -Force -Path $tmp | Out-Null
        $init = Join-Path $tmp 'rustup-init.exe'
        Get-Pinned $m['rustup']['url'] $m['rustup']['sha256'] $init
        Invoke-Checked 'rustup-init' { & $init -y --no-modify-path --profile minimal --default-toolchain none }
        Say 'done' "rustup: $rustup"
        Say 'note' "ignore rustup's hint 'rustup default stable': the toolchain step installs the pinned version"
    }
    # rustup stays at the pinned version: no self-update (it also used to fail on a locked rustup-init.exe)
    if ($Apply) { Invoke-Checked 'rustup set auto-self-update' { rustup set auto-self-update disable } }
}

function Step-Git {
    $git = Get-Command git -ErrorAction SilentlyContinue
    if ($git) {
        Say 'ok' "git: $($git.Source)"
    } elseif (-not $Apply) {
        Say 'plan' "PortableGit (sha256 pinned, with Git Bash) -> $portGit"
    } else {
        New-Item -ItemType Directory -Force -Path $tmp, $portGit | Out-Null
        $sfx = Join-Path $tmp 'PortableGit.7z.exe'
        Get-Pinned $m['portablegit']['url'] $m['portablegit']['archive_sha256'] $sfx
        # a 7-Zip self-extracting archive of git-for-windows: -o<folder> -y unpacks without questions
        $p = Start-Process -FilePath $sfx -ArgumentList @("-o`"$portGit`"", '-y') -Wait -PassThru -WindowStyle Hidden
        Remove-Item -LiteralPath $sfx -Force -ErrorAction SilentlyContinue
        if ($p.ExitCode -ne 0 -or -not (Test-Path -LiteralPath "$portGit\cmd\git.exe")) { throw "PortableGit did not unpack (exit $($p.ExitCode))" }
        $env:PATH = "$portGit\cmd;" + $env:PATH
        Say 'done' "git: $portGit\cmd\git.exe"
    }
}

function Step-Clone {
    if (Test-Path -LiteralPath (Join-Path $ws 'config\workspace.toml')) { Say 'ok' "workspace: $((Test-Step 'clone')[1])"; return }
    if (-not $Apply) {
        $how = if (Test-Partial) { 'partial clone (base only)' } else { 'git clone' }
        Say 'plan' "$how $Repo -> $ws, checkout $Ref"; return
    }
    if ((Test-Path -LiteralPath $ws) -and (Get-ChildItem -LiteralPath $ws -Force | Select-Object -First 1)) {
        Fail "$ws exists, is not empty and has no config\workspace.toml (a broken clone?) - look inside, delete it, run the step again"
    }
    if (Test-Partial) {
        # files arrive only for the folders on disk; the rest of git history stays without file contents until needed
        Invoke-Checked "git clone --filter=blob:none --sparse $Repo (is the repository reachable? a private one needs a sign-in or a key)" { git clone --quiet --filter=blob:none --sparse $Repo $ws }
    } else {
        Invoke-Checked "git clone $Repo (is the repository reachable? a private one needs a sign-in or a key)" { git clone --quiet $Repo $ws }
    }
    if ($Ref) { Invoke-Checked "git checkout $Ref" { git -C $ws -c advice.detachedHead=false checkout --quiet $Ref } }
    if ($Ref -match '^[0-9a-f]{40}$') {
        $head = (git -C $ws rev-parse HEAD).Trim()
        if ($head -ne $Ref) { Fail "HEAD is $head, expected $Ref" }
    }
    if (Test-Partial) { Set-BaseSparse }
    Say 'done' "workspace: $((Test-Step 'clone')[1])"
}

function Step-Toolchain {
    if (-not $Apply) { Say 'plan' 'rustup toolchain install (version from rust-toolchain.toml)'; return }
    In-Workspace {
        # rustup does not install the version from rust-toolchain.toml by itself (rust-lang/rustup#4836)
        Invoke-Checked 'rustup toolchain install (network? run the step again)' { rustup toolchain install }
        Say 'ok' ((rustc --version | Out-String).Trim())
    }
}

function Step-Linker {
    if (-not $Apply) {
        $vs = Find-VS
        if ($Linker -ne 'xwin' -and $vs) { Say 'ok' "linker: Visual Studio Build Tools ($vs)" }
        else { Say 'plan' 'linker: cargo-xwin (Microsoft CRT + Windows SDK, Microsoft license) into home\cache\xwin, about 1 GB' }
        return
    }
    if (-not (Use-Xwin)) { Say 'ok' "linker: Visual Studio Build Tools ($(Find-VS))"; return }
    In-Workspace {
        $xw = Join-Path $dbHome 'tools\cargo-xwin'
        $xwExe = Join-Path $xw 'cargo-xwin.exe'
        if (-not ((Test-Path -LiteralPath $xwExe) -and (Get-Sha256 $xwExe) -eq $m['cargo-xwin']['sha256'])) {
            New-Item -ItemType Directory -Force -Path $tmp | Out-Null
            $zip = Join-Path $tmp 'cargo-xwin.zip'
            Get-Pinned $m['cargo-xwin']['url'] $m['cargo-xwin']['archive_sha256'] $zip
            Expand-Archive2 $zip $xw
            if ((Get-Sha256 $xwExe) -ne $m['cargo-xwin']['sha256']) { Fail 'cargo-xwin.exe: sha256 does not match the manifest' }
        }
        # cargo-xwin calls the linker lld-link; rust-lld from the toolchain is the same LLVM lld and picks its mode by name
        $sysroot = ((rustc --print sysroot) | Out-String).Trim()
        Copy-Item -LiteralPath (Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe') -Destination (Join-Path $xw 'lld-link.exe') -Force
        $env:XWIN_CACHE_DIR = Join-Path $dbHome 'cache\xwin'
        $x = Join-Path $env:XWIN_CACHE_DIR 'xwin'
        if (-not (Test-Path -LiteralPath "$x\crt\lib\x86_64")) {
            Say 'note' 'cargo-xwin: downloading Microsoft CRT and Windows SDK (Microsoft license), about 1 GB'
            & { $ErrorActionPreference = 'Continue'; & $xwExe env --target x86_64-pc-windows-msvc | Out-Null }
            if (-not (Test-Path -LiteralPath "$x\crt\lib\x86_64")) { Fail "cargo-xwin did not produce $x (network? run the step again)" }
        }
        if ((Test-Path -LiteralPath $cargoConfig) -and -not ((Get-Content -LiteralPath $cargoConfig -Raw) -match [regex]::Escape($xwinMarker))) {
            Fail "$cargoConfig exists and was not written by bootstrap - not touching it"
        }
        $f = { param($p) $p -replace '\\', '/' }
        $xs = & $f $x
        $llvm = & $f (Join-Path $dbHome 'toolchains\llvm\bin')
        $inc = "$xs/crt/include;$xs/sdk/include/ucrt;$xs/sdk/include/um;$xs/sdk/include/shared;$xs/sdk/include/winrt"
        $lib = "$xs/crt/lib/x86_64;$xs/sdk/lib/um/x86_64;$xs/sdk/lib/ucrt/x86_64"
        $imsvc = ($inc -split ';' | ForEach-Object { "/imsvc $_" }) -join ' '
        $cflags = "--target=x86_64-pc-windows-msvc -Wno-unused-command-line-argument -fuse-ld=lld-link $imsvc"
        $cfg = @"
$xwinMarker
# Build without Visual Studio (docs/WORKSPACE_PLAN.md, section 13): Microsoft CRT + Windows SDK from cargo-xwin,
# linker = rust-lld (as lld-link), C/C++ and resources = LLVM (cargo xtask setup installs it into toolchains\llvm).
# Rewritten by bootstrap.ps1; delete this file to go back to Visual Studio.
[target.x86_64-pc-windows-msvc]
linker = "$(& $f (Join-Path $xw 'lld-link.exe'))"

[env]
DBDEV_XWIN = "1"
LIB = "$lib"
INCLUDE = "$inc"
CC_x86_64_pc_windows_msvc = "$llvm/clang-cl.exe"
CXX_x86_64_pc_windows_msvc = "$llvm/clang-cl.exe"
AR_x86_64_pc_windows_msvc = "$llvm/llvm-lib.exe"
RC_x86_64_pc_windows_msvc = "$llvm/llvm-rc.exe"
CFLAGS_x86_64_pc_windows_msvc = "$cflags"
CXXFLAGS_x86_64_pc_windows_msvc = "$cflags /EHsc"
"@
        [IO.File]::WriteAllText($cargoConfig, $cfg, (New-Object Text.UTF8Encoding $false))
        Say 'done' "linker: cargo-xwin (no Visual Studio), settings in $cargoConfig"
        # [env] from the config reaches cargo's child processes; this shell needs DBDEV_XWIN for cargo xtask as well
        $env:DBDEV_XWIN = '1'
    }
}

function Step-Tools {
    # always the official releases (sha256 pinned), whatever -Build says: see .PARAMETER Build
    if (-not $Apply) { Say 'plan' "cargo xtask setup --only=tools --apply --prebuilt  (tools.lock -> home\tools, sha256 pinned)"; return }
    $a = @('xtask', 'setup', '--only=tools', '--apply', '--prebuilt')
    if ($Tool) { $a += "--tool=$Tool" }
    In-Workspace { Invoke-Checked 'cargo xtask setup --only=tools (a download failed? run the step again; -Tool <name> for one tool)' { & cargo @a } }
}

function Step-Devkit {
    if (-not $Apply) { Say 'plan' "cargo xtask setup --only=ours --apply --$Build  (ws and DevKit: signed dist or built here)"; return }
    $flag = if ($Build -eq 'source') { '--from-source' } else { '--prebuilt' }
    In-Workspace { Invoke-Checked 'cargo xtask setup --only=ours' { cargo xtask setup --only=ours --apply $flag } }
}

function Step-Components {
    if (-not (Test-Partial)) { Say 'ok' 'components: everything on disk (a full clone; -Components <preset> for a part)'; return }
    $names = ($Components -split '[,\s]+' | Where-Object { $_ }) -join ','
    if (-not $Apply) { Say 'plan' "ws components set $names --apply  (the chosen components and what they need)"; return }
    $lt = Join-Path $ws 'config\local.toml'
    if (-not (Test-Path -LiteralPath $lt)) { Copy-Item -LiteralPath (Join-Path $ws 'config\local.example.toml') -Destination $lt }
    $wsExe = Join-Path $dbHome 'tools\ws\ws.exe'
    In-Workspace {
        if (Test-Path -LiteralPath $wsExe) { Invoke-Checked 'ws components set' { & $wsExe components set $names --apply } }
        else { Invoke-Checked 'cargo ws components set' { cargo ws components set $names --apply } }
    }
}

# ------------------------------------------------------------------------------------------------ run
if ($Status) {
    Write-Host "Dungeonborne DevKit bootstrap - status (nothing changes)"
    Say 'root' $Root
    $next = $null
    foreach ($s in $Steps) {
        $r = Test-Step $s
        $tag = if ($r[0]) { 'done' } else { 'todo' }
        if (-not $r[0] -and $s -ne 'machine' -and -not $next) { $next = $s }
        Write-Host ('{0,-6} {1,-10} {2}' -f $tag, $s, $r[1])
    }
    if ($next) { Say 'next' "bootstrap.ps1 -Root $Root$(Get-Extra) -Step $next -Apply   (or without -Step: all remaining steps)" }
    else { Say 'next' 'nothing - bootstrap is complete' }
    exit 0
}

$mode = if ($Apply) { 'apply' } else { 'plan (nothing changes; add -Apply to do it)' }
Write-Host "Dungeonborne DevKit bootstrap - $mode$(if ($Step -ne 'all') { ", step $Step" })"
Say 'root' $Root
Say 'repo' "$Repo @ $Ref"
Say 'build' $Build

$run = if ($Step -eq 'all') { $Steps } else { @($Step) }
if ($Step -ne 'all' -and $Apply) {
    # one step: the earlier ones must be done (machine is only a check)
    $i = $Steps.IndexOf($Step)
    for ($k = 1; $k -lt $i; $k++) {
        $b = $Steps[$k]
        if (-not (Test-Step $b)[0]) { Fail "step '$Step' needs step '$b' first: bootstrap.ps1 -Root $Root$(Get-Extra) -Step $b -Apply" }
    }
    # the folders of the layout, as the machine step makes them: step by step, machine never runs with -Apply (it is
    # always "done"), and <root>\vault was missing (19.11, the newcomer test)
    foreach ($d in $Root, $vault, $dbHome, "$dbHome\toolchains", "$dbHome\tools", $tmp) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
}
foreach ($s in $run) {
    $script:Current = $s
    if ($Step -eq 'all') { Write-Host "== $s" }
    switch ($s) {
        'machine' { Step-Machine }
        'rustup' { Step-Rustup }
        'git' { Step-Git }
        'clone' { Step-Clone }
        'toolchain' { Step-Toolchain }
        'linker' { Step-Linker }
        'tools' { Step-Tools }
        'devkit' { Step-Devkit }
        'components' { Step-Components }
    }
}
$script:Current = ''

if (-not $Apply) {
    Write-Host ''
    Write-Host 'Nothing changed. Read the plan, then run again with -Apply.'
    exit 0
}
if ($Step -ne 'all' -and $Step -ne 'devkit' -and $Step -ne 'components') {
    Write-Host ''
    Write-Host "Step $Step done. Next: bootstrap.ps1 -Root $Root$(Get-Extra) -Status"
    exit 0
}
# home\env.ps1: the environment of this workspace for a new PowerShell window (dot-source it). The user environment
# stays untouched; without it cargo is not found, or another Rust on the machine is used (17.11, trial install).
$envScript = Join-Path $dbHome 'env.ps1'
$cargoBin = Join-Path $env:CARGO_HOME 'bin'
# programs from tools.lock that builds call by name (trunk -> tailwindcss for the launcher and Studio, cargo tauri)
$toolDirs = (@('tauri-cli\bin', 'trunk\bin', 'tailwindcss', 'uv') | ForEach-Object { Join-Path $dbHome "tools\$_" }) -join ';'
$envText = @"
# Generated by bootstrap.ps1. In a new PowerShell window, before cargo / cargo ws / cargo tauri commands:
#   . '$envScript'
# Sets the Rust and the build programs of this workspace for this window only (the user environment is not changed).
`$env:RUSTUP_HOME = '$env:RUSTUP_HOME'
`$env:CARGO_HOME = '$env:CARGO_HOME'
`$env:CARGO_TARGET_DIR = '$env:CARGO_TARGET_DIR'
`$env:DB_HOME = '$dbHome'
if (-not (`$env:PATH -like '$cargoBin;*')) { `$env:PATH = '$cargoBin;$toolDirs;' + `$env:PATH }
# JDK from tools.lock (cargo xtask setup --tool=jdk) for Ghidra; without it nothing changes
if (Test-Path -LiteralPath '$dbHome\toolchains\jdk\bin\java.exe') { `$env:JAVA_HOME = '$dbHome\toolchains\jdk' }
# Python of this workspace (tools.lock: python, put by ws bootstrap through uv) before a Python of the machine
`$py = Get-ChildItem -Directory -LiteralPath '$dbHome\toolchains\python' -Filter 'cpython-3.*-none' -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -Last 1
if (`$py -and (Test-Path -LiteralPath (Join-Path `$py.FullName 'python.exe'))) { `$env:DB_PYTHON = Join-Path `$py.FullName 'python.exe'; if (-not (`$env:PATH -like "`$(`$py.FullName);*")) { `$env:PATH = `$py.FullName + ';' + (Join-Path `$py.FullName 'Scripts') + ';' + `$env:PATH } }
Set-Location -LiteralPath '$ws'
"@
[IO.File]::WriteAllText($envScript, $envText, (New-Object System.Text.UTF8Encoding $true))
Write-Host ''
Write-Host 'Bootstrap finished. Next:'
Write-Host "  1. In a new PowerShell window first:  . '$envScript'"
Write-Host "     (Rust of this workspace for that window only: RUSTUP_HOME, CARGO_HOME, CARGO_TARGET_DIR, PATH)"
Write-Host "  2. cargo ws doctor --profile=dev   (read-only: what the next command will install)"
Write-Host "     cargo ws bootstrap --profile=dev   (installs: config\local.toml, home folders, ~1 GB of tools; profiles: dev, designer, agent)"
Write-Host "     team member: docs\CONTRIBUTING.md, 'Setup from scratch' - secrets and data from the artifact store come here"
Write-Host "  3. cargo ws check   - 0 errors; cargo ws doctor - one expected error, [game] reference, until your copy of the game is set up (README)"
Write-Host "  4. DevKit: $(Get-DevkitExe)"
Write-Host "  5. AI (Claude Code): open it in $ws - not in the folder above it; DevKit 'Start' does that for you"
if (Test-Partial) {
    Write-Host "  Components on disk: cargo ws components list; more: cargo ws components add <name> --apply (docs\COMPONENTS_PLAN.md)"
}
