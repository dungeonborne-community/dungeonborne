# sandbox — experiments

A place to try things freely: throwaway scripts, copies of code to play with, trial builds, notes, your own
`git init` for an experiment. Only this README is in git; everything else here is ignored (`.gitignore`) and stays on
your machine. Decision — `docs/adr/0003-sandbox.md`.

## Inside `sandbox/` an AI agent may, without asking

- create, change, move and delete files and folders here (only here);
- copy code from members here and change the copies;
- keep its own git repositories here, commit in them, reset them;
- run builds, tests and scripts on what is here.

`ws check` does not look inside, and nothing here needs a branch, a commit or a pull request.

## What still applies here

All rules of `CLAUDE.md` that are not about this repository's files:

- secrets are never printed or saved here; they reach a command only through `ws secrets run`;
- nothing goes outside: no push, no publishing, no messages, no deploys;
- the anti-cheat is not bypassed, game memory is not read, game processes are not killed;
- unpacking paks with the key and analysing the INTL login — only after the human's explicit word;
- files outside `sandbox/` are changed only the normal way (in their member, with `ws check`).

## The border

- Nothing in members may refer to `sandbox/` — `ws check` warns when member code does. It is not in git: a
  teammate does not have it.
- A result worth keeping moves out: into the member it belongs to (`docs/EXTENDING.md` — where new things go), then
  `ws check`, a commit on a branch, a pull request (`docs/CONTRIBUTING.md`).
- The folder can be emptied any time; keep nothing here that exists nowhere else and matters.
