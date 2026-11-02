using System;
using System.IO;

internal static class Program
{
    internal static readonly string DirectoryPath = Path.Combine(Path.GetTempPath(), "CapRecordingTests-" + Guid.NewGuid().ToString("N"));
    private static int assertions;
    [STAThread]
    private static int Main(string[] args)
    {
        if (args.Length == 2 && args[0] == "--journal-child") return SessionTests.JournalChild(args[1]);
        Directory.CreateDirectory(DirectoryPath);
        try
        {
            Run("Partial-region GIF uses direct capture and preserves selected dimensions", SessionTests.GifRegionCapture);
            Run("GIF LZW palette, delta rectangles and identical frame merging", EncoderTests.GifRoundTrip);
            Run("GIF 60 FPS timing has no cumulative rounding drift", EncoderTests.GifTiming);
            Run("GIF blue gradients preserve local color averages", EncoderTests.GifGradient);
            Run("MP4 H264 roundtrip dimensions, colors and orientation", EncoderTests.Mp4RoundTrip);
            Run("Interrupted compressed journal recovers complete frames", SessionTests.Recovery);
            Run("A failed recovery can be retried to the same output", SessionTests.RecoveryRetry);
            Run("Killed process journal recovers to decodable MP4", SessionTests.CrashRecovery);
            Run("Output dimensions and invalid options", SessionTests.Options);
            Run("Native cursor snapshot, coordinate text, click pulse and resize pixels", OverlayTests.Run);
            Run("Pause removes wall time; repeated Stop is safe", SessionTests.Lifecycle);
            Run("Closing the captured window finalizes its recording", SessionTests.TargetCloses);
            Run("Closing a paused target finalizes without resuming", SessionTests.PausedTargetCloses);
            Console.WriteLine($"PASS: {assertions} assertions. Artifacts: {DirectoryPath}");
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine(error);
            Console.Error.WriteLine("Artifacts: " + DirectoryPath);
            return 1;
        }
    }
    private static void Run(string name, Action action) { action(); Console.WriteLine("PASS: " + name); }
    internal static void Assert(bool value, string message)
    {
        assertions++;
        if (!value) throw new InvalidOperationException(message);
    }
}
