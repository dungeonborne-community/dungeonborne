# game/client-mod — client mod installer

`install.ps1` (and `install.bat`, which starts it) prepares a game client for a community server:

- asks for **administrator rights** (restarts itself elevated);
- copies the mod paks next to it into the game's `Paks` folder;
- replaces the game's `cacert.pem` with the community one (the original is kept as a backup);
- adds lines to the Windows `hosts` file (backup `hosts.backup`) that send the game's former service domains to
  `127.0.0.1`, where the community backend or its relay answers.

Read the script before running it, and run it only on your own machine with your own game copy. The launcher
(`apps/launcher`) does the same for players with a UAC prompt; newer launchers need no `hosts` lines at all when the
server enables `[relay] no_hosts`.
