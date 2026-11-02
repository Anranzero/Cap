using System.Windows;
using System.Windows.Media.Imaging;
using Cap.Capture;
using Cap.Core;

namespace Cap.Annotation;

public enum CaptureMode { Screenshot, Scrolling, Recording, Text }
public sealed record ScreenshotResult(PixelRect Bounds, BitmapSource Image, string? SavedPath = null, bool Copied = false, bool Pinned = false, bool RecordRequested = false, CaptureMode Mode = CaptureMode.Screenshot)
{
    /// <summary>The mode bar's top-left corner in absolute physical screen pixels.</summary>
    public Point? ModeBarPosition { get; init; }

    /// <summary>Format chosen in the overlay's recording panel (Feishu-style start card).</summary>
    public Cap.Core.RecordingFormat RecordingFormat { get; init; } = Cap.Core.RecordingFormat.Mp4;

    /// <summary>The stitched image produced by the in-place scrolling capture.</summary>
    public BitmapSource? ScrollImage { get; init; }
}

public static class ScreenshotWorkflow
{
    /// <summary>
    /// Shows the overlay non-modally so the scrolling capture bar can stay interactive above it,
    /// and completes when the overlay closes.
    /// </summary>
    public static Task<ScreenshotResult?> RunAsync(string defaultSaveDirectory, bool darkTheme = false, CaptureMode initialMode = CaptureMode.Screenshot)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(defaultSaveDirectory);
        return OnUiThread(() => Show(defaultSaveDirectory, darkTheme, false, initialMode)).Unwrap();
    }

    public static Task<PixelRect?> SelectRegionAsync(bool darkTheme = false)
        => OnUiThread(() => ShowRegion(darkTheme)).Unwrap();

    private static Task<ScreenshotResult?> Show(string directory, bool darkTheme, bool regionOnly, CaptureMode initialMode)
    {
        PixelRect desktop = CaptureService.GetVirtualBounds();
        IReadOnlyList<DesktopWindow> windows = CaptureService.GetWindows();
        PixelFrame frame = CaptureService.Capture(desktop);
        return ShowLoop(desktop, frame, windows, directory, darkTheme, regionOnly, initialMode, preselect: false);
    }

    /// <summary>
    /// Runs one overlay at a time; the crop tool closes the overlay and asks for a
    /// second, smaller one covering the cropped region (Feishu's window-shrinking crop).
    /// </summary>
    private static async Task<ScreenshotResult?> ShowLoop(PixelRect desktop, PixelFrame frame, IReadOnlyList<DesktopWindow> windows,
        string directory, bool darkTheme, bool regionOnly, CaptureMode initialMode, bool preselect)
    {
        while (true)
        {
            var window = new ScreenshotWindow(desktop, frame, windows, directory, darkTheme, regionOnly, initialMode, preselect);
            var completion = new TaskCompletionSource<ScreenshotResult?>(TaskCreationOptions.RunContinuationsAsynchronously);
            window.Closed += (_, _) => completion.TrySetResult(window.Result);
            window.Show();
            window.Activate();
            ScreenshotResult? result = await completion.Task;
            if (window.CropRegion is { } region && window.CroppedFrame is { } cropped && region.Width >= 8 && region.Height >= 8)
            {
                desktop = region;
                frame = cropped;
                windows = Array.Empty<DesktopWindow>();
                initialMode = CaptureMode.Screenshot;
                preselect = true;
                continue;
            }
            return result;
        }
    }

    private static Task<PixelRect?> ShowRegion(bool darkTheme)
    {
        var window = CreateWindow(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), darkTheme, true);
        var completion = new TaskCompletionSource<PixelRect?>(TaskCreationOptions.RunContinuationsAsynchronously);
        window.Closed += (_, _) => completion.TrySetResult(window.RegionResult);
        window.Show();
        window.Activate();
        return completion.Task;
    }

    private static ScreenshotWindow CreateWindow(string directory, bool darkTheme, bool regionOnly, CaptureMode initialMode = CaptureMode.Screenshot)
    {
        PixelRect desktop = CaptureService.GetVirtualBounds();
        var windows = CaptureService.GetWindows();
        PixelFrame frame = CaptureService.Capture(desktop);
        return new ScreenshotWindow(desktop, frame, windows, directory, darkTheme, regionOnly, regionOnly ? CaptureMode.Recording : initialMode);
    }

    private static Task<T> OnUiThread<T>(Func<T> action)
    {
        if (Application.Current is null) throw new InvalidOperationException("截图工具需要正在运行的 WPF Application。");
        if (Application.Current.Dispatcher.CheckAccess()) return Task.FromResult(action());
        return Application.Current.Dispatcher.InvokeAsync(action).Task;
    }
}
