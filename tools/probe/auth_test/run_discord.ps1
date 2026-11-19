# Backend with Discord login for a local check. The secret is taken from a system variable (not printed).
$env:DISCORD_CLIENT_SECRET = [Environment]::GetEnvironmentVariable('DISCORD_CLIENT_SECRET', 'Machine')
if (-not $env:DISCORD_CLIENT_SECRET) { Write-Host 'DISCORD_CLIENT_SECRET is not set' -ForegroundColor Red; exit 1 }
$env:SERVER_TOML = 'probe/auth_test/server_discord.toml'
$env:DB_PATH = 'probe/auth_test/db'
$env:RPC_DUMP = '1'
cargo run
