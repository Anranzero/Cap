using System;
using System.IO;
using System.Diagnostics;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Cap.Core;
using Cap.Capture;
using Cap.Recording;
using static Program;
using static EncoderTests;

internal static class SessionTests
{
    internal static void GifRegionCapture()
    {
        var bounds = new PixelRect(20, 30, 160, 100);
        var options = new RecordingOptions { Bounds = bounds, Format = RecordingFormat.Gif, FramesPerSecond = 10,
            OutputPath = Path.Combine(DirectoryPath, "region-capture.gif"), ShowCursor = false, ShowCoordinates = false, HighlightClicks = false };
        using var session = new RecordingSession(options);
        using var first = new ManualResetEventSlim();
        string? backend = null;
        session.Progress += (_, progress) => { backend = progress.Backend; first.Set(); };
        session.Start();
        Assert(first.Wait(TimeSpan.FromSeconds(10)), "Partial GIF must produce a real frame.");
        Thread.Sleep(250);
        var result = session.StopAsync().GetAwaiter().GetResult();
        var decoded = DecodeGif(result.FilePath);
        Assert(backend == "GDI", "Partial GIF must capture its region directly without a monitor-wide WGC indicator. Actual: " + backend);
        Assert(decoded.Frames[0].PixelWidth == 160 && decoded.Frames[0].PixelHeight == 100, "Direct GIF capture must preserve the selected 160x100 dimensions.");
        Assert(RecordingSession.UsesDirectRegionCapture(options with { Bounds = new(-200, -150, 160, 100) }), "A signed partial region must use the same capture policy.");
        Assert(!RecordingSession.UsesDirectRegionCapture(options with { Format = RecordingFormat.Mp4 }), "MP4 must preserve its WGC preference.");
        Assert(!RecordingSession.UsesDirectRegionCapture(options with { WindowHandle = 1 }), "Selected-window GIF must preserve its WGC preference.");
        foreach (PixelRect monitor in CaptureService.GetMonitors())
            Assert(!RecordingSession.UsesDirectRegionCapture(options with { Bounds = monitor }), "A complete monitor GIF must preserve its WGC preference.");
    }

    internal static void Recovery()
    {
        string journal = Path.Combine(DirectoryPath, "interrupted.gif.capjournal");
        var header = new JournalHeader(1, "interrupted.gif", DateTime.UtcNow, 8, 8, 10, RecordingFormat.Gif);
        using (var writer = new FrameJournal(journal, header))
        {
            writer.Write(Solid(8, 8, 255, 0, 0), 0, 1_000_000);
            writer.Write(Solid(8, 8, 0, 0, 255), 1_000_000, 1_000_000);
        }
        using (var stream = new FileStream(journal, FileMode.Append)) stream.Write(new byte[] { 9, 8, 7 });
        File.WriteAllBytes(Path.Combine(DirectoryPath, "invalid.capjournal"), new byte[] { 1, 2, 3 });
        var candidates = RecordingRecovery.FindCandidates(DirectoryPath);
        Assert(candidates.Any(item => item.JournalPath == journal), "Recoverable journal must be discoverable.");
        string output = Path.Combine(DirectoryPath, "recovered.gif");
        var result = RecordingRecovery.RecoverAsync(journal, output).GetAwaiter().GetResult();
        Assert(result.FrameCount == 2 && result.Duration == TimeSpan.FromMilliseconds(200), "Recovery must preserve committed frames and ignore an incomplete final record.");
        Assert(DecodeGif(output).Frames.Count == 2 && File.Exists(journal), "Recovery must decode and preserve its source journal.");
    }
    internal static void Options()
    {
        var options = new RecordingOptions { Bounds = new(0, 0, 1920, 1080), OutputWidth = 1280, OutputPath = Path.Combine(DirectoryPath, "options.mp4") };
        Assert(RecordingSession.ResolveDimensions(options) == (1280, 720), "Width-only output should preserve aspect ratio.");
        Assert(RecordingSession.ResolveDimensions(options with { OutputWidth = 3840 }) == (1920, 1080), "Output should not upscale capture.");
        Assert(RecordingSession.ResolveDimensions(options with { OutputWidth = 721 }) == (720, 406), "H264 output dimensions should be even.");
        try { using var session = new RecordingSession(options with { FramesPerSecond = 17 }); throw new Exception("Invalid MP4 FPS was accepted."); }
        catch (ArgumentException) { Assert(true, "Invalid FPS rejected."); }
        try { using var session = new RecordingSession(options with { CoordinateColor = null! }); throw new Exception("Null color was accepted."); }
        catch (ArgumentException) { Assert(true, "Null color rejected."); }
    }

    internal static void RecoveryRetry()
    {
        string journal = Path.Combine(DirectoryPath, "retry.gif.capjournal");
        using (var writer = new FrameJournal(journal, new JournalHeader(1, "retry.gif", DateTime.UtcNow, 8, 8, 10, RecordingFormat.Gif)))
            writer.Write(Solid(8, 8, 255, 0, 0), 0, 1_000_000);
        long validLength = new FileInfo(journal).Length;
        using (var file = new FileStream(journal, FileMode.Append)) file.Write(new byte[20]);
        string output = Path.Combine(DirectoryPath, "retry-recovered.gif");
        try { RecordingRecovery.RecoverAsync(journal, output).GetAwaiter().GetResult(); throw new Exception("Corrupt record was accepted."); }
        catch (InvalidDataException) { Assert(File.Exists(journal) && !File.Exists(output), "Failed recovery must preserve its journal and not publish a damaged file."); }
        using (var file = new FileStream(journal, FileMode.Open, FileAccess.Write)) file.SetLength(validLength);
        var result = RecordingRecovery.RecoverAsync(journal, output).GetAwaiter().GetResult();
        Assert(result.FrameCount == 1 && DecodeGif(output).Frames.Count == 1, "Recovery retry to the same output should succeed.");
    }
    internal static void Lifecycle()
    {
        string path = Path.Combine(DirectoryPath, "session.gif");
        var options = new RecordingOptions { Bounds = new(0, 0, 160, 100), Format = RecordingFormat.Gif, FramesPerSecond = 10,
            OutputPath = path, ShowCursor = false, ShowCoordinates = false, HighlightClicks = false };
        using var session = new RecordingSession(options);
        using var firstFrame = new ManualResetEventSlim();
        session.Progress += (_, progress) => { if (progress.FrameCount > 0) firstFrame.Set(); };
        session.Start();
        Assert(firstFrame.Wait(TimeSpan.FromSeconds(10)), "Real screen recording must produce a first frame.");
        Thread.Sleep(250);
        session.Pause();
        Assert(session.State == RecordingState.Paused, "Pause must change state.");
        Thread.Sleep(600);
        session.Resume();
        Thread.Sleep(250);
        Task<RecordingResult> first = session.StopAsync(), second = session.StopAsync();
        Assert(ReferenceEquals(first, second), "Repeated StopAsync calls must share completion.");
        var result = first.GetAwaiter().GetResult();
        Assert(result.Duration.TotalMilliseconds is >= 400 and < 1000, "Paused wall time must be excluded from output duration.");
        Assert(session.State == RecordingState.Completed && File.Exists(path), "Completed recording must publish the file.");
        Assert(!File.Exists(path + ".capjournal") && !File.Exists(path + ".partial.gif"), "Successful completion should clean staging files.");
        Assert(DecodeGif(path).Frames.Count > 0, "Real capture output must decode.");
    }

    internal static int JournalChild(string path)
    {
        var header = new JournalHeader(1, "killed.mp4", DateTime.UtcNow, 160, 96, 15, RecordingFormat.Mp4);
        using var journal = new FrameJournal(path, header);
        for (int i = 0; ; i++)
        {
            journal.Write(Solid(160, 96, (byte)(i % 2 * 255), 0, (byte)((i + 1) % 2 * 255)), i * 10_000_000L / 15, (i + 1) * 10_000_000L / 15 - i * 10_000_000L / 15);
            if (i == 20) File.WriteAllText(path + ".ready", "ready");
            Thread.Sleep(70);
        }
    }

    internal static void TargetCloses() => TargetCloses(paused: false);
    internal static void PausedTargetCloses() => TargetCloses(paused: true);
    private static void TargetCloses(bool paused)
    {
        using var form = new System.Windows.Forms.Form { Text = "Cap recording close test", Bounds = new System.Drawing.Rectangle(150, 150, 320, 240) };
        form.Show();
        System.Windows.Forms.Application.DoEvents();
        var options = new RecordingOptions { Bounds = new(150, 150, 320, 240), WindowHandle = form.Handle,
            Format = RecordingFormat.Gif, FramesPerSecond = 10, OutputPath = Path.Combine(DirectoryPath, paused ? "paused-closed-window.gif" : "closed-window.gif"),
            ShowCursor = false, ShowCoordinates = false, HighlightClicks = false };
        using var session = new RecordingSession(options);
        using var first = new ManualResetEventSlim();
        session.Progress += (_, _) => first.Set();
        session.Start();
        Assert(first.Wait(TimeSpan.FromSeconds(10)), "Window capture must start.");
        if (paused) session.Pause();
        form.Close();
        Assert(SpinWait.SpinUntil(() => session.Completion.IsCompleted, TimeSpan.FromSeconds(10)), "Closing target should finish recording.");
        var result = session.Completion.GetAwaiter().GetResult();
        Assert(session.State == RecordingState.Completed && result.FrameCount >= 1 && File.Exists(options.OutputPath), "Closing target must save even its first pending frame.");
        Assert(DecodeGif(options.OutputPath).Frames.Count >= 1, "Saved closed-window GIF must decode.");
    }

    internal static void CrashRecovery()
    {
        string journal = Path.Combine(DirectoryPath, "killed.mp4.capjournal");
        string host = Path.GetFullPath(Path.Combine(RuntimeEnvironment.GetRuntimeDirectory(), "..", "..", "..", "dotnet.exe"));
        var start = new ProcessStartInfo(host) { UseShellExecute = false, CreateNoWindow = true };
        start.ArgumentList.Add(typeof(Program).Assembly.Location);
        start.ArgumentList.Add("--journal-child");
        start.ArgumentList.Add(journal);
        using var child = Process.Start(start) ?? throw new InvalidOperationException("Could not launch journal crash test.");
        try
        {
            Assert(SpinWait.SpinUntil(() => File.Exists(journal + ".ready") || child.HasExited, TimeSpan.FromSeconds(10)) && !child.HasExited, "Child process must have flushed its recording journal.");
            child.Kill(entireProcessTree: true);
            child.WaitForExit();
            string output = Path.Combine(DirectoryPath, "killed-recovered.mp4");
            var result = RecordingRecovery.RecoverAsync(journal, output).GetAwaiter().GetResult();
            var decoded = Mp4Decoder.Decode(output);
            Assert(result.FrameCount >= 1 && decoded.Count == result.FrameCount, $"Every committed frame must survive process termination: journal={result.FrameCount}, decoded={decoded.Count}.");
            Assert(decoded.Width == 160 && decoded.Height == 96 && decoded.Subtype == MfNative.H264, "Crash recovery must produce a normal H264 MP4.");
        }
        finally { if (!child.HasExited) { child.Kill(entireProcessTree: true); child.WaitForExit(); } }
    }
}
