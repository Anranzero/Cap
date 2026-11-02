using System.Diagnostics;
using System.Runtime.InteropServices;
using Cap.Core;
using Cap.Capture;
using Cap.Recording;

string dir = args.Length > 0 ? args[0] : Path.Combine(Path.GetTempPath(), "cap-rec-measure");
Directory.CreateDirectory(dir);
int width = int.Parse(args.Length > 1 ? args[1] : "2560");
int height = int.Parse(args.Length > 2 ? args[2] : "1440");
int seconds = int.Parse(args.Length > 3 ? args[3] : "5");
string mode = args.Length > 4 ? args[4] : "record";
var bounds = new PixelRect(0, 0, width, height);

if (mode == "capture")
{
    bool preferWgc = bool.Parse(Environment.GetEnvironmentVariable("PREFER_WGC") ?? "true");
    using var source = CaptureService.CreateSource(new CaptureRequest(bounds, 0, PreferGraphicsCapture: preferWgc));
    var watch = Stopwatch.StartNew();
    int frames = 0;
    while (watch.Elapsed < TimeSpan.FromSeconds(seconds)) { source.Capture(); frames++; }
    Console.WriteLine($"capture-only backend={source.BackendName} frames={frames} fps={frames / watch.Elapsed.TotalSeconds:F1} msPerFrame={watch.Elapsed.TotalMilliseconds / frames:F1}");
    return;
}

bool Env(string name, bool fallback) => bool.Parse(Environment.GetEnvironmentVariable(name) ?? fallback.ToString());
bool overlay = Env("SHOW_OVERLAY", true);
string path = Path.Combine(dir, "measure.mp4");
if (File.Exists(path)) File.Delete(path);
using var session = new RecordingSession(new RecordingOptions
{
    Bounds = bounds,
    Format = RecordingFormat.Mp4,
    FramesPerSecond = 60,
    ShowCursor = Env("SHOW_CURSOR", overlay),
    HighlightClicks = Env("SHOW_CLICKS", overlay),
    ShowCoordinates = Env("SHOW_COORDS", overlay),
    OutputPath = path
});
long dropped = 0, reported = 0;
string backend = "";
session.Progress += (_, p) => { dropped = p.DroppedFrames; reported = p.FrameCount; backend = p.Backend; };

int wiggle = 0;
if (Env("WIGGLE", false))
{
    wiggle = 1;
    _ = Task.Run(async () =>
    {
        int t = 0;
        while (Volatile.Read(ref wiggle) == 1)
        {
            Native.SetCursorPos(200 + (int)(Math.Sin(t / 6.0) * 900) + 900, 300 + (int)(Math.Cos(t / 9.0) * 400) + 300);
            t++;
            await Task.Delay(8);
        }
    });
}

session.Start();
await Task.Delay(TimeSpan.FromSeconds(seconds));
var result = await session.StopAsync();
Volatile.Write(ref wiggle, 0);
Console.WriteLine($"backend={backend} frames={result.FrameCount} dropped={dropped} reportedFrames={reported} fps={(result.FrameCount / result.Duration.TotalSeconds):F1} droppedPct={(100.0 * dropped / Math.Max(1, dropped + result.FrameCount)):F1}% size={new FileInfo(path).Length / 1024 / 1024}MB cursor=({Env("SHOW_CURSOR", overlay)},{Env("SHOW_COORDS", overlay)},{Env("SHOW_CLICKS", overlay)}) wiggle={Env("WIGGLE", false)}");

static class Native
{
    [DllImport("user32.dll")] internal static extern bool SetCursorPos(int x, int y);
}
