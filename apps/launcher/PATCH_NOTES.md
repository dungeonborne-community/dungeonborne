# Launcher release notes (for Discord)

Each release is a separate post; a Discord post is up to 2000 characters, formatted with ordinary Discord Markdown.
Not published yet.

---

## Launcher 1.3.0

Updates itself: launcher 1.2.0 will show an "Update and restart" bar.

**New Windows 11-style design**
• The window looks like a Windows 11 app: navigation on the left, a server page with a big "Play" button, settings on a page of their own, like Windows "Settings".
• Under the "Play" button it says why you cannot play yet: not signed in, the exe does not match, and so on.
• Fonts and icons are the system ones of Windows, nothing is downloaded.
• You can suggest your own design: UI Sandbox in ✨・plugin-showcase.

---

## Launcher 1.2.0

**This version has to be downloaded by hand once** — after that the launcher updates itself.

**Mods now come from the server**
• The launcher downloads the server's mods before you play and installs them only for the game session — after you exit, the game folder is clean again.
• When a mod is updated on the server, the launcher fetches the new version by itself; a new launcher is no longer needed for that.
• Only what changed is downloaded; the download progress shows in the launch window.
• Files of earlier launcher versions are removed from the game automatically. Files you changed yourself are left alone.

**"Play" in Steam opens the launcher**
• Settings → "Open the launcher with Steam's 'Play' button". The launcher takes the place of the game's bootstrap; the original is kept next to it and put back when you uncheck the box.
• Starting from Steam gives no Windows warnings, and the anti-cheat does not mind.
• After Steam's "Verify integrity of game files" the launcher takes its place again on its next start.
• Pressing "Play" again while the launcher is open just brings its window up.

**Launcher updates**
• When a new version is out, an "Update and restart" bar appears at the top.
• The launcher installs only updates with our digital signature — they cannot be swapped.
• An update never starts while you are in the game.

**Small things**
• The mod with native code (dbnative) asks for permission once, not before every game.
• Settings → "Mod cache": how much space it takes, and a "Clear" button.
• Before the Windows "Publisher: unknown" prompt (the one-time hosts setup) the launcher explains what it is and why.
• Debug mods and the CheatManager switch are removed from the client.

**If something goes wrong** — write in the support channel and attach the file `%LOCALAPPDATA%\DungeonborneLauncher\logs\launcher.log`.
