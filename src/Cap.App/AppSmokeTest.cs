using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using System.Windows.Media.Imaging;
using Cap.Core;
using Cap.Capture;
using Cap.Recording;

namespace Cap.App;

/// <summary>Explicit command-line integration check; never runs during normal tray startup.</summary>
internal static class AppSmokeTest
{
    public static async Task RunAsync(string[] args)
    {
        int index = Array.IndexOf(args, "--smoke-test");
        string directory = index + 1 < args.Length ? Path.GetFullPath(args[index + 1]) : Path.Combine(Path.GetTempPath(), "cap-smoke-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory); var results = new List<string>(); bool passed = false;
        try
        {
            PixelRect desktop = CaptureService.GetVirtualBounds();
            PixelRect bounds = new(desktop.X + 20, desktop.Y + 20, Math.Min(640, desktop.Width - 20), Math.Min(360, desktop.Height - 20));
            var pixels = CaptureService.Capture(bounds); pixels.Validate();
            using (var file = File.Create(Path.Combine(directory, "screenshot.png")))
            {
                var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(CaptureService.ToBitmapSource(pixels))); encoder.Save(file);
            }
            results.Add($"Desktop screenshot {pixels.Width}x{pixels.Height}");
            foreach (var format in new[] { RecordingFormat.Mp4, RecordingFormat.Gif })
            {
                string path = Path.Combine(directory, format == RecordingFormat.Mp4 ? "recording.mp4" : "recording.gif");
                using var session = new RecordingSession(new RecordingOptions { Bounds = bounds, Format = format, FramesPerSecond = 15, ShowCoordinates = true, ShowCursor = true, HighlightClicks = true, OutputPath = path });
                var ready = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
                string? backend = null;
                session.Progress += (_, progress) => { backend = progress.Backend; ready.TrySetResult(); };
                session.Start(); await ready.Task.WaitAsync(TimeSpan.FromSeconds(15));
                await Task.Delay(1200); session.Pause(); await Task.Delay(250); session.Resume(); await Task.Delay(600);
                var result = await session.StopAsync();
                if (result.FrameCount < 2 || !File.Exists(path) || new FileInfo(path).Length < 200) throw new InvalidOperationException("Recorder did not create real frames: " + format);
                if (format == RecordingFormat.Gif)
                {
                    if (backend != "GDI") throw new InvalidOperationException("Partial-region GIF must use direct GDI capture: " + backend);
                    using var stream = File.OpenRead(path); var decoder = new GifBitmapDecoder(stream, BitmapCreateOptions.PreservePixelFormat, BitmapCacheOption.OnLoad);
                    if (decoder.Frames.Count < 1) throw new InvalidOperationException("GIF contained no decodable frames.");
                    if (decoder.Frames[0].PixelWidth != bounds.Width || decoder.Frames[0].PixelHeight != bounds.Height)
                        throw new InvalidOperationException("GIF dimensions do not match the selected region.");
                    if (decoder.Frames[0].Metadata is not BitmapMetadata metadata || Convert.ToInt32(metadata.GetQuery("/grctlext/Delay")) <= 0)
                        throw new InvalidOperationException("GIF animation timing metadata is missing.");
                    results.Add($"GIF native decode {decoder.Frames.Count} frames");
                }
                results.Add($"{format} recording {result.FrameCount} frames, {result.Duration.TotalSeconds:F3}s, {new FileInfo(path).Length} bytes, {backend}");
            }
            passed = true;
        }
        catch (Exception ex) { results.Add(ex.ToString()); }
        await File.WriteAllTextAsync(Path.Combine(directory, "smoke-result.json"), JsonSerializer.Serialize(new { Passed = passed, Results = results, Runtime = Environment.Version.ToString(), Process = Environment.ProcessPath }, new JsonSerializerOptions { WriteIndented = true }));
        Environment.ExitCode = passed ? 0 : 1;
    }
}
