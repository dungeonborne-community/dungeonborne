# Preview: seeing the result

Every piece of work ends with the human seeing it with their own eyes, in one step (`agents/ROLES.md`, report part
"See it"). For a member that shows something — a site, an app's UI, a page — that step is its `[preview]`.

## `[preview]` in member.toml

```toml
[preview]
name = "site"                                  # the name people and agents use (default: the member folder name)
run  = ["python", "web/dev_server.py", "8821"] # a server that keeps running and builds what it needs itself
dir  = "."                                     # where run starts, from the member folder (default ".")
port = 8821
path = "/"                                     # the page to open (default "/")
note = "the site from the files, as on the hosting"   # what it shows — and what it does not
env  = { NO_COLOR = "true" }                   # optional environment
```

- `run` is a list (program and arguments), no shell. If the result needs a build, `run` does it (`trunk serve`):
  a preview that shows yesterday's build misleads.
- `note` says honestly what the person sees: "the UI in a browser with a mock of the Tauri API — not the real window"
  is the right note for an app.
- Names and ports are unique across the workspace (`ws check`).
- `ws check` warns about a `site` or `app` member without `[preview]`.

## Using it

| Who | How |
|---|---|
| Human | DevKit, page "Work" → "See the result" → "Open": a terminal with the server, and the browser once it answers |
| Human, console | `cargo ws preview` (the list), `cargo ws preview <name> --open`; Ctrl+C stops it |
| AI agent in Claude Code | the preview tool with the same name — `.claude/launch.json` is generated from `[preview]` |

It shows the current checkout: the branch DevKit shows on the same page.

`.claude/launch.json` is generated: `cargo ws preview --sync --apply` puts every `[preview]` in (an entry with the
same name is replaced, other entries stay). `ws check` warns when it is out of step. Do not edit those entries by
hand — edit `[preview]`.

## Current previews

`cargo ws preview` lists them. On 18.11: `site` (web/landing), `launcher-ui` (apps/launcher), `balance-studio-ui`
(apps/studio), `devkit-ui` (apps/devkit).
