using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cap.Core;

namespace Cap.Capture;

/// <summary>Desktop capture in physical pixels; returned frames never include the native cursor.</summary>
public static class CaptureService
{
    /// <summary>Development preview only. Set before creating windows to include cap UI in screenshots.</summary>
    public static bool IncludeOwnWindows { get; set; }

    public static PixelRect GetVirtualBounds()
    {
        using var dpi = new NativeMethods.DpiScope();
        return new(NativeMethods.GetSystemMetrics(76), NativeMethods.GetSystemMetrics(77),
            NativeMethods.GetSystemMetrics(78), NativeMethods.GetSystemMetrics(79));
    }

    public static IReadOnlyList<PixelRect> GetMonitors() => GetMonitorHandles().Select(item => item.Bounds).ToArray();

    internal static IReadOnlyList<(nint Handle, PixelRect Bounds)> GetMonitorHandles()
    {
        using var dpi = new NativeMethods.DpiScope();
        var monitors = new List<(nint Handle, PixelRect Bounds)>();
        if (!NativeMethods.EnumDisplayMonitors(0, 0, (nint monitor, nint dc, ref NativeMethods.Rect rect, nint data) =>
            {
                monitors.Add((monitor, FromNative(rect)));
                return true;
            }, 0))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot enumerate monitors.");
        return monitors;
    }

    public static IReadOnlyList<DesktopWindow> GetWindows()
    {
        using var dpi = new NativeMethods.DpiScope();
        var windows = new List<DesktopWindow>();
        if (!NativeMethods.EnumWindows((window, _) =>
            {
                var candidate = DescribeWindow(window);
                if (candidate is not null) windows.Add(candidate);
                return true;
            }, 0))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot enumerate desktop windows.");
        return windows;
    }

    public static DesktopWindow? GetWindowAt(int x, int y)
    {
        using var dpi = new NativeMethods.DpiScope();
        nint window = NativeMethods.WindowFromPoint(new() { X = x, Y = y });
        window = NativeMethods.GetAncestor(window, 2);
        return DescribeWindow(window) ?? GetWindows().FirstOrDefault(candidate => candidate.Bounds.Contains(x, y));
    }

    internal static DesktopWindow? DescribeWindow(nint window)
    {
        if (window == 0 || !NativeMethods.IsWindowVisible(window) || NativeMethods.IsIconic(window) ||
            (NativeMethods.GetWindowLongPtrW(window, -20).ToInt64() & 0x80) != 0)
            return null;
        if (NativeMethods.DwmGetWindowAttributeInt(window, 14, out int cloaked, sizeof(int)) == 0 && cloaked != 0)
            return null;
        if (NativeMethods.GetWindowDisplayAffinity(window, out uint affinity) && affinity != 0)
            return null;
        int length = NativeMethods.GetWindowTextLengthW(window);
        if (length <= 0) return null;
        var title = new StringBuilder(length + 1);
        if (NativeMethods.GetWindowTextW(window, title, title.Capacity) == 0) return null;
        if (!TryGetWindowBounds(window, out PixelRect bounds)) return null;
        return bounds.IsEmpty ? null : new(window, title.ToString(), bounds);
    }

    internal static PixelRect GetWindowBounds(nint window)
    {
        using var dpi = new NativeMethods.DpiScope();
        if (!NativeMethods.IsWindow(window)) throw new CaptureTargetClosedException();
        if (!TryGetWindowBounds(window, out PixelRect bounds))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot read the selected window bounds.");
        return bounds;
    }

    private static bool TryGetWindowBounds(nint window, out PixelRect bounds)
    {
        bounds = default;
        if (NativeMethods.DwmGetWindowAttribute(window, 9, out NativeMethods.Rect rect, (uint)Marshal.SizeOf<NativeMethods.Rect>()) != 0 ||
            rect.Right <= rect.Left || rect.Bottom <= rect.Top)
        {
            if (!NativeMethods.GetWindowRect(window, out rect)) return false;
        }
        bounds = FromNative(rect);
        return true;
    }

    /// <summary>Captures the visible desktop immediately into an independent, top-down BGRA buffer.</summary>
    public static PixelFrame Capture(PixelRect bounds)
    {
        using var source = new GdiCaptureSource(bounds);
        return source.Capture();
    }

    /// <summary>
    /// Uses WGC for a window or a region wholly on one monitor; otherwise uses GDI.
    /// Output dimensions remain fixed for the source lifetime. Window WGC Bounds follows the
    /// live physical window origin, cropping or black-padding its top-left corner after a resize. GDI samples
    /// the original desktop rectangle, including occluding windows.
    /// </summary>
    public static ICaptureSource CreateSource(CaptureRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);
        ValidateBounds(request.Bounds);
        if (request.WindowHandle != 0 && !NativeMethods.IsWindow(request.WindowHandle))
            throw new CaptureTargetClosedException();
        if (request.PreferGraphicsCapture)
        {
            try
            {
                var source = GraphicsCaptureSource.TryCreate(request);
                if (source is not null) return new ResilientCaptureSource(request.Bounds, request.WindowHandle, source);
            }
            catch (Exception error) when (IsGraphicsFailure(error))
            {
                Trace.TraceWarning("Windows Graphics Capture unavailable; using GDI: {0}", error);
            }
        }
        var fallback = new GdiCaptureSource(request.Bounds);
        return request.WindowHandle == 0 ? fallback : new ResilientCaptureSource(request.Bounds, request.WindowHandle, fallback);
    }

    /// <summary>Excludes an owned top-level window, such as a selection overlay, from capture.</summary>
    public static void ExcludeWindow(nint window)
    {
        if (window == 0) throw new ArgumentException("A valid top-level window handle is required.", nameof(window));
        uint affinity = IncludeOwnWindows ? 0u : NativeMethods.WdaExcludeFromCapture;
        if (!NativeMethods.SetWindowDisplayAffinity(window, affinity))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot update the window capture policy.");
    }

    public static BitmapSource ToBitmapSource(PixelFrame frame)
    {
        ArgumentNullException.ThrowIfNull(frame);
        frame.Validate();
        var bitmap = BitmapSource.Create(frame.Width, frame.Height, 96, 96, PixelFormats.Bgra32, null, frame.Pixels, frame.Stride);
        bitmap.Freeze();
        return bitmap;
    }

    internal static void ValidateBounds(PixelRect bounds)
    {
        if (bounds.IsEmpty) throw new ArgumentOutOfRangeException(nameof(bounds), "Capture dimensions must be positive.");
        _ = bounds.Right;
        _ = bounds.Bottom;
        _ = checked(bounds.Width * bounds.Height * 4);
    }

    internal static bool IsGraphicsFailure(Exception error) => error is not CaptureTargetClosedException &&
        error is (COMException or Win32Exception or NotSupportedException or InvalidOperationException or TimeoutException or ArgumentException);

    private static PixelRect FromNative(NativeMethods.Rect rect) =>
        new(rect.Left, rect.Top, checked(rect.Right - rect.Left), checked(rect.Bottom - rect.Top));

    private sealed class ResilientCaptureSource(PixelRect bounds, nint windowHandle, ICaptureSource initial) : ICaptureSource
    {
        private readonly object gate = new();
        private ICaptureSource source = initial;
        private bool disposed;
        public PixelRect Bounds { get { lock (gate) return source.Bounds; } }
        public string BackendName { get { lock (gate) return source.BackendName; } }
        public PixelFrame Capture()
        {
            lock (gate)
            {
                ObjectDisposedException.ThrowIf(disposed, this);
                if (windowHandle != 0 && !NativeMethods.IsWindow(windowHandle))
                    throw new CaptureTargetClosedException();
                try { return source.Capture(); }
                catch (Exception error) when (source is GraphicsCaptureSource graphics && !graphics.TargetClosed && IsGraphicsFailure(error))
                {
                    Trace.TraceWarning("Windows Graphics Capture failed; switching to GDI: {0}", error);
                    source.Dispose();
                    source = new GdiCaptureSource(bounds);
                    return source.Capture();
                }
            }
        }
        public void Dispose()
        {
            lock (gate)
            {
                if (disposed) return;
                disposed = true;
                source.Dispose();
            }
        }
    }
}
