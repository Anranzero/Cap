using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using Cap.Core;

namespace Cap.Annotation;

internal static partial class NativeDesktop
{
    [StructLayout(LayoutKind.Sequential)] private struct NativePoint { public int X; public int Y; }
    [StructLayout(LayoutKind.Sequential)] private struct NativeRect { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)] private struct MonitorInfo { public int Size; public NativeRect Monitor; public NativeRect Work; public uint Flags; }
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetCursorPos(out NativePoint point);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetWindowPos(nint hwnd, nint after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] private static extern nint MonitorFromPoint(NativePoint point, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetMonitorInfo(nint monitor, ref MonitorInfo info);

    public static Point Cursor()
    {
        if (!GetCursorPos(out NativePoint point)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        return new Point(point.X, point.Y);
    }

    public static void Place(Window window, PixelRect bounds)
    {
        nint handle = new WindowInteropHelper(window).Handle;
        if (!SetWindowPos(handle, new nint(-1), bounds.X, bounds.Y, bounds.Width, bounds.Height, 0x0040))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
    }

    public static PixelRect MonitorAt(Point point)
    {
        var native = new NativePoint { X = (int)point.X, Y = (int)point.Y };
        var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
        if (!GetMonitorInfo(MonitorFromPoint(native, 2), ref info))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        return new(info.Monitor.Left, info.Monitor.Top, info.Monitor.Right - info.Monitor.Left, info.Monitor.Bottom - info.Monitor.Top);
    }

    /// <summary>The monitor work area, falling back to the full monitor when the query fails.</summary>
    public static PixelRect WorkAreaAt(Point point)
    {
        var native = new NativePoint { X = (int)point.X, Y = (int)point.Y };
        var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
        if (!GetMonitorInfo(MonitorFromPoint(native, 2), ref info)) return MonitorAt(point);
        return new(info.Work.Left, info.Work.Top, info.Work.Right - info.Work.Left, info.Work.Bottom - info.Work.Top);
    }
}
