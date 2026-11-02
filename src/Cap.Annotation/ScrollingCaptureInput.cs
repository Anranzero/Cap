using System.ComponentModel;
using System.Runtime.InteropServices;
using Cap.Core;

namespace Cap.Annotation;

/// <summary>Sends one downward wheel step to the original content window without moving the cursor.</summary>
internal sealed class ScrollingCaptureInput
{
    private const uint MouseWheel = 0x020A;
    private const uint RootWindow = 2;
    private const int WheelStep = -120;
    private readonly NativePoint point;
    private readonly nint originalRoot;

    public ScrollingCaptureInput(PixelRect region)
    {
        point = new NativePoint { X = region.X + region.Width / 2, Y = region.Y + region.Height / 2 };
        originalRoot = GetAncestor(WindowFromPoint(point), RootWindow);
    }

    public void ScrollDown()
    {
        nint target = WindowFromPoint(point);
        if (originalRoot == 0 || target == 0 || GetAncestor(target, RootWindow) != originalRoot || !IsWindowVisible(originalRoot))
            throw new InvalidOperationException("目标窗口已被遮挡或关闭，请将它移回选区后用鼠标滚轮继续截取。");
        if (point.X is < short.MinValue or > short.MaxValue || point.Y is < short.MinValue or > short.MaxValue)
            throw new InvalidOperationException("此屏幕位置不支持自动滚动，请使用鼠标滚轮。");
        nint position = new((point.X & 0xffff) | ((point.Y & 0xffff) << 16));
        if (!PostMessageW(target, MouseWheel, new nint(WheelStep << 16), position))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "无法自动滚动此窗口，请使用鼠标滚轮。");
    }

    public bool ActivateTarget() => originalRoot != 0 && SetForegroundWindow(originalRoot);

    [StructLayout(LayoutKind.Sequential)] private struct NativePoint { public int X; public int Y; }
    [DllImport("user32.dll")] private static extern nint WindowFromPoint(NativePoint point);
    [DllImport("user32.dll")] private static extern nint GetAncestor(nint window, uint flags);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool IsWindowVisible(nint window);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetForegroundWindow(nint window);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool PostMessageW(nint window, uint message, nint wParam, nint lParam);
}
