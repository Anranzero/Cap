using System.Diagnostics;
using Cap.Core;
using Cap.Capture;
using Cap.Recording;
using System.Diagnostics.Tracing;
Trace.Listeners.Add(new ConsoleTraceListener());

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
    while (watch.Elapsed < TimeSpan.FromSeconds(seconds))
    {
        var frame = source.Capture();
        frames++;
    }
    Console.WriteLine($"capture-only backend={source.BackendName} frames={frames} fps={frames / watch.Elapsed.TotalSeconds:F1} msPerFrame={watch.Elapsed.TotalMilliseconds / frames:F1}");
    return;
}

string path = Path.Combine(dir, "measure.mp4");
if (File.Exists(path)) File.Delete(path);
bool overlay = bool.Parse(Environment.GetEnvironmentVariable("SHOW_OVERLAY") ?? "true");
using var session = new RecordingSession(new RecordingOptions
{
    Bounds = bounds,
    Format = RecordingFormat.Mp4,
    FramesPerSecond = 60,
    ShowCursor = overlay,
    HighlightClicks = overlay,
    ShowCoordinates = overlay,
    OutputPath = path
});
long dropped = 0, framesReported = 0;
string backend = "";
session.Progress += (_, p) => { dropped = p.DroppedFrames; framesReported = p.FrameCount; backend = p.Backend; };
session.Start();
await Task.Delay(TimeSpan.FromSeconds(seconds));
var result = await session.StopAsync();
Console.WriteLine($"backend={backend} frames={result.FrameCount} dropped={dropped} reportedFrames={framesReported} fps={(result.FrameCount / result.Duration.TotalSeconds):F1} droppedPct={(100.0 * dropped / Math.Max(1, dropped + result.FrameCount)):F1}% size={new FileInfo(path).Length / 1024 / 1024}MB");
