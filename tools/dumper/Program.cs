// dbdump — game assets -> JSON and PNG over CUE4Parse (docs/GAME_DATA.md). Run by `ws dump`, not by hand:
//
//   dbdump --paks <Content\Paks> --usmap <file.usmap> --oodle <oo2core_9_win64.dll> --out <dir> --list <patterns.txt>
//          [--game GAME_UE5_1]
//
// The pak key comes only from the environment variable GAME_AES_KEY (never from the command line, never printed).
// patterns.txt: one line per pattern, "png <glob>" or "json <glob>"; * matches any characters including '/'.
// Output: <out>/<package path without the first folder>.{json,png} — the layout of an FModel export
// (Content/TGF/...), and <out>/index.tsv: format, package, result. Packages are processed in sorted order and every
// file is written the same way on every run, so two runs give the same bytes.
// No downloads: Oodle comes from --oodle (the one retoc installed by tools.lock), textures are decoded by the managed
// decoder (no native detex), PNG by SkiaSharp from NuGet.
using System.Collections.Concurrent;
using System.IO.Enumeration;
using System.Text;
using CUE4Parse.Compression;
using CUE4Parse.Encryption.Aes;
using CUE4Parse.FileProvider;
using CUE4Parse.MappingsProvider.Usmap;
using CUE4Parse.UE4.Assets.Exports.Texture;
using CUE4Parse.UE4.Objects.Core.Misc;
using CUE4Parse.UE4.Versions;
using CUE4Parse_Conversion.Options;
using CUE4Parse_Conversion.Textures;
using Newtonsoft.Json;

static class Program
{
    static int Main(string[] args)
    {
        var opt = new Dictionary<string, string>();
        for (var i = 0; i + 1 < args.Length; i += 2)
        {
            if (!args[i].StartsWith("--")) return Fail($"unexpected argument: {args[i]}");
            opt[args[i][2..]] = args[i + 1];
        }
        foreach (var need in new[] { "paks", "usmap", "oodle", "out", "list" })
            if (!opt.ContainsKey(need)) return Fail($"missing --{need} (see the header of Program.cs)");
        var key = Environment.GetEnvironmentVariable("GAME_AES_KEY") ?? "";
        if (key.Length == 0) return Fail("no GAME_AES_KEY in the environment (ws secrets run game_aes_key -- ...)");
        if (!key.StartsWith("0x", StringComparison.OrdinalIgnoreCase)) key = "0x" + key;
        if (!Enum.TryParse<EGame>(opt.GetValueOrDefault("game", "GAME_UE5_1"), out var game))
            return Fail($"unknown --game {opt["game"]}");

        var patterns = new List<(string Format, string Glob)>();
        foreach (var raw in File.ReadAllLines(opt["list"]))
        {
            var line = raw.Trim();
            if (line.Length == 0 || line.StartsWith('#')) continue;
            var sp = line.IndexOf(' ');
            var fmt = sp > 0 ? line[..sp] : "";
            if (fmt != "png" && fmt != "json") return Fail($"list: '{line}' - expected 'png <glob>' or 'json <glob>'");
            patterns.Add((fmt, line[(sp + 1)..].Trim()));
        }

        OodleHelper.Initialize(opt["oodle"]);
        TextureDecoder.UseAssetRipperTextureDecoder = true; // managed BC7/ETC decoders: no native detex download
        var provider = new DefaultFileProvider(opt["paks"], SearchOption.TopDirectoryOnly, new VersionContainer(game),
            StringComparer.OrdinalIgnoreCase)
        {
            MappingsContainer = new FileUsmapTypeMappingsProvider(opt["usmap"]),
        };
        provider.Initialize();
        provider.SubmitKey(new FGuid(), new FAesKey(key));
        provider.PostMount();
        Console.Error.WriteLine($"mounted: {provider.Files.Count} files");

        // which packages, with which format (the first matching line wins); sorted for a stable order
        var jobs = new SortedDictionary<string, string>(StringComparer.Ordinal);
        foreach (var file in provider.Files.Values)
        {
            var path = file.Path;
            if (!path.EndsWith(".uasset", StringComparison.OrdinalIgnoreCase)) continue;
            foreach (var (fmt, glob) in patterns)
            {
                if (FileSystemName.MatchesSimpleExpression(glob, path, ignoreCase: true))
                {
                    jobs.TryAdd(path, fmt);
                    break;
                }
            }
        }
        Console.Error.WriteLine($"matched: {jobs.Count} packages");

        var outDir = opt["out"];
        var results = new ConcurrentDictionary<string, string>();
        var json = new JsonSerializer { Formatting = Formatting.Indented };
        Parallel.ForEach(jobs, new ParallelOptions { MaxDegreeOfParallelism = Environment.ProcessorCount }, job =>
        {
            var (path, fmt) = (job.Key, job.Value);
            var rel = path.IndexOf('/') is var cut and > 0 ? path[(cut + 1)..] : path; // drop "Dungeonborne/"
            var target = Path.Combine(outDir, Path.ChangeExtension(rel, fmt).Replace('/', Path.DirectorySeparatorChar));
            try
            {
                if (!provider.TryLoadPackage(path, out var pkg))
                {
                    results[path] = "error: not loaded";
                    return;
                }
                Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                if (fmt == "json")
                {
                    using var w = new StreamWriter(target, false, new UTF8Encoding(false)) { NewLine = "\n" };
                    json.Serialize(w, pkg.GetExports());
                    results[path] = "ok";
                    return;
                }
                var tex = pkg.GetExports().OfType<UTexture>().FirstOrDefault();
                if (tex is null)
                {
                    results[path] = "skip: no texture";
                    return;
                }
                var decoded = tex.Decode();
                if (decoded is null)
                {
                    results[path] = "error: texture not decoded";
                    return;
                }
                File.WriteAllBytes(target, decoded.Encode(ETextureFormat.Png, false, out _));
                results[path] = "ok";
            }
            catch (Exception e)
            {
                results[path] = "error: " + e.GetType().Name + ": " + e.Message.Replace('\t', ' ').Replace('\n', ' ');
            }
        });

        var index = new StringBuilder();
        foreach (var (path, fmt) in jobs)
            index.Append(fmt).Append('\t').Append(path).Append('\t').Append(results.GetValueOrDefault(path, "error: no result")).Append('\n');
        Directory.CreateDirectory(outDir);
        File.WriteAllText(Path.Combine(outDir, "index.tsv"), index.ToString(), new UTF8Encoding(false));
        var errors = results.Values.Count(r => r.StartsWith("error"));
        var ok = results.Values.Count(r => r == "ok");
        Console.Error.WriteLine($"done: {ok} ok, {results.Count - ok - errors} skipped, {errors} errors -> {outDir}");
        return errors == 0 ? 0 : 3;
    }

    static int Fail(string msg)
    {
        Console.Error.WriteLine("dbdump: " + msg);
        return 2;
    }
}
