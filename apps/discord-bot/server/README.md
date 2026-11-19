# Discord server setup

`server.yaml` describes the server structure (roles, categories, channels, forums with tags, permissions, onboarding, welcome message and rules).
`setup_server.py` brings the server in line with that description through the Discord API. The script deletes nothing: it only creates what is missing and updates what the config describes. It is safe to run repeatedly.

The scripts live in `apps/discord-bot/server`; the exported message texts are kept in `apps/discord-bot/server/texts` (`export_texts.py` may be configured to write elsewhere).

## 1. Bot

1. https://discord.com/developers/applications → **New Application** → **Bot** tab → **Reset Token**, copy the token.
2. **OAuth2 → URL Generator**: scopes `bot`, permissions `Administrator` → open the link and add the bot to the server.
3. In Discord, enable Developer Mode (Settings → Advanced), right-click the server → **Copy Server ID**.

`Administrator` is only needed during setup. Afterwards you can take the permissions away from the bot or remove it from the server.

## 2. Running

```powershell
python -m venv .venv
.\.venv\Scripts\python -m pip install -r requirements.txt
copy .env.example .env      # fill in DISCORD_TOKEN and DISCORD_GUILD_ID
```

```powershell
.\.venv\Scripts\python setup_server.py --offline   # check the config, without Discord
.\.venv\Scripts\python setup_server.py             # plan of changes — the server is not modified
.\.venv\Scripts\python setup_server.py --apply     # apply
```

In the output: `+` will be created, `~` will be changed, `=` already matches the config, `!` warning.

Before `--apply`, replace the `TODO` links in `messages.welcome` in `server.yaml`. Messages are only posted into an empty channel: to republish, delete the old one and run again.

## 3. What to finish by hand

The bot API cannot do these:

- **Move the bot role to the top.** Server Settings → Roles → drag the bot role above all others. Otherwise it cannot change existing roles, sort roles or save onboarding (response `403 Missing Permissions`).
- **Onboarding.** Discord requires 7 default channels, 5 of them open to everyone where `@everyone` can post. The config meets this requirement. If Discord still refuses, the script saves the questions disabled. Then: Server Settings → Onboarding → check the channels and enable it.
- **Server Guide**: Server Settings → Onboarding.
- **AutoMod**: Server Settings → AutoMod (phishing links, mention spam).
- **GitHub webhook** for `📦・framework-releases`: Channel Settings → Integrations → Webhooks, URL + `/github` in the repository settings.
- **Bots** (optional): reaction translator, Ticket Tool for private support, a server status bot for `🌐・server-status`.
- **Post templates** in forums are not supported by the API. The template is in each forum's description (guidelines).

## Discord permission limitation

Role permissions combine with OR: you cannot open a channel only to those who have both `Player` and `EN` at the same time. Therefore:
- language chats and LFG (`chat-*`, `lfg-*`, `Lounge *`) are in COMMUNITY and are unlocked by language;
- `dev-chat-en` / `dev-chat-ru` are visible to all developers;
- forums are shared; the post language is set by the 🇬🇧 EN / 🇷🇺 RU tag.
