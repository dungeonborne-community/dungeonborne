# tools/dumper — dbdump

Game assets → files you can read: icons → PNG, monster and hero abilities, effects and data assets, curves → JSON.
Not run by hand: `cargo ws dump` builds it and runs it (`tools/ws/core/src/dump.rs`, `docs/GAME_DATA.md`).

| File | What |
|---|---|
| `Program.cs` | the whole tool (~150 lines): mount the paks, match package paths, write JSON / PNG and `index.tsv` |
| `dbdump.csproj` | .NET 10, CUE4Parse and CUE4Parse-Conversion `1.2.2.202610` (FabianFG, Apache-2.0), Newtonsoft.Json |
| `packages.lock.json` | every NuGet package with its hash; restore takes only these |
| `sets.toml` | what `ws dump` exports: sets `icons`, `monsters`, `heroes`, `curves` (patterns of package paths) |

## How it works

- Input: the game reference `home/game/original/<build>/Dungeonborne/Content/Paks`, the usmap from `home/re/ue4ss`,
  Oodle from `home/tools/retoc`. The pak key — only the environment variable `GAME_AES_KEY` (never a command line).
- Output: `home/dumps/<build>/export/Content/TGF/...` — the same layout as an FModel export, so scripts written for
  FModel exports work (`tools/py/item_icons.py --exports <home>/dumps/<build>/export`).
- Same input → same bytes: packages in sorted order, JSON with `\n`, no times or random names.
- No downloads at run time. CLI tools on the same library download Oodle, zlib and detex DLLs from the internet on
  every start; here Oodle comes from retoc (installed by `tools.lock`), textures are decoded by the managed decoder
  (`TextureDecoder.UseAssetRipperTextureDecoder`), PNG by SkiaSharp from NuGet.

## Known

- NuGet warns about vulnerabilities in `SixLabors.ImageSharp 3.1.12` (pinned exactly by CUE4Parse-Conversion) and
  `Microsoft.Bcl.Memory 9.0.0`. dbdump reads only the game's own paks from the person's Steam, not files from strangers;
  bump CUE4Parse when a release moves them.
- Large virtual textures (UE5 LFS) are not supported by CUE4Parse; UI icons are ordinary textures.
- Update CUE4Parse: the version in `dbdump.csproj`, then `dotnet restore --force-evaluate` (rewrites
  `packages.lock.json`), then a dump and a comparison of `manifest.tsv` with the previous one.
