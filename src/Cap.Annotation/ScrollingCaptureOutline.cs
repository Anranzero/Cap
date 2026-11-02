using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;
using Cap.Capture;
using Cap.Core;

namespace Cap.Annotation;

/// <summary>Input-transparent edges outside the captured pixels keep the scrolling region visible.</summary>
internal sealed class ScrollingCaptureOutline : IDisposable
{
    private readonly PixelRect[] strips;
    private readonly List<EdgeWindow> edges = [];
    private bool shown, disposed;

    public ScrollingCaptureOutline(PixelRect region)
    {
        if (region.IsEmpty) throw new ArgumentOutOfRangeException(nameof(region));
        const int thickness = 2;
        int left = checked(region.X - thickness), top = checked(region.Y - thickness);
        int width = checked(region.Width + thickness * 2);
        _ = checked(region.Right + thickness); _ = checked(region.Bottom + thickness);
        strips =
        [
            new(left, top, width, thickness),
            new(left, region.Bottom, width, thickness),
            new(left, region.Y, thickness, region.Height),
            new(region.Right, region.Y, thickness, region.Height)
        ];
    }

    public void Show()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (shown) return;
        try
        {
            foreach (PixelRect strip in strips)
            {
                var edge = new EdgeWindow(strip);
                edges.Add(edge);
                edge.Show();
            }
            shown = true;
        }
        catch (Exception creationError)
        {
            try { Dispose(); }
            catch (Exception cleanupError) { throw new AggregateException("Cannot create or clean up the scrolling capture outline.", creationError, cleanupError); }
            throw;
        }
    }

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        List<Exception>? errors = null;
        foreach (var edge in edges)
        {
            try { edge.Close(); }
            catch (Exception error) { (errors ??= []).Add(error); }
        }
        edges.Clear();
        if (errors != null) throw new AggregateException("Cannot close every scrolling capture edge.", errors);
    }

    private sealed class EdgeWindow : Window
    {
        private const int ExtendedStyle = -20;
        private const long TransparentNoActivateToolWindow = 0x080000A0L;
        private readonly PixelRect physicalBounds;
        private HwndSource? source;

        public EdgeWindow(PixelRect bounds)
        {
            physicalBounds = bounds;
            Title = "cap · 滚动截图范围";
            WindowStyle = WindowStyle.None; ResizeMode = ResizeMode.NoResize; AllowsTransparency = true;
            Background = new SolidColorBrush(Color.FromRgb(8, 125, 255));
            ShowActivated = false; ShowInTaskbar = false; Topmost = true; Focusable = false; IsHitTestVisible = false;
            WindowStartupLocation = WindowStartupLocation.Manual;
            Left = bounds.X; Top = bounds.Y; Width = bounds.Width; Height = bounds.Height;
            SourceInitialized += (_, _) =>
            {
                nint handle = new WindowInteropHelper(this).Handle;
                nint style = GetWindowLongPtrW(handle, ExtendedStyle);
                if (style == 0 && Marshal.GetLastWin32Error() != 0) throw new Win32Exception(Marshal.GetLastWin32Error());
                nint previous = SetWindowLongPtrW(handle, ExtendedStyle, new nint(style.ToInt64() | TransparentNoActivateToolWindow));
                if (previous == 0 && Marshal.GetLastWin32Error() != 0) throw new Win32Exception(Marshal.GetLastWin32Error());
                CaptureService.ExcludeWindow(handle);
                source = HwndSource.FromHwnd(handle); source?.AddHook(WindowMessage);
                Place();
            };
            Loaded += (_, _) => Place();
            Closed += (_, _) => source?.RemoveHook(WindowMessage);
        }

        private nint WindowMessage(nint hwnd, int message, nint wParam, nint lParam, ref bool handled)
        {
            if (message == 0x0084) { handled = true; return new nint(-1); } // HTTRANSPARENT
            if (message == 0x0021) { handled = true; return new nint(3); } // MA_NOACTIVATE
            if (message == 0x02E0)
                Dispatcher.BeginInvoke(DispatcherPriority.Loaded, new Action(() => { if (IsVisible) Place(); }));
            return 0;
        }

        private void Place()
        {
            if (!SetWindowPos(new WindowInteropHelper(this).Handle, new nint(-1), physicalBounds.X, physicalBounds.Y,
                physicalBounds.Width, physicalBounds.Height, 0x0010))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot position the scrolling capture edge.");
        }

        [DllImport("user32.dll", SetLastError = true)] private static extern nint GetWindowLongPtrW(nint handle, int index);
        [DllImport("user32.dll", SetLastError = true)] private static extern nint SetWindowLongPtrW(nint handle, int index, nint value);
        [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetWindowPos(nint handle, nint after, int x, int y, int width, int height, uint flags);
    }
}
