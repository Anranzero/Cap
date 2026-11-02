using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Cap.Core;

namespace Cap.Recording;

/// <summary>Draws the actual cursor through a top-down GDI DIB, including monochrome AND/XOR cursors.</summary>
internal static class CursorRenderer
{
    internal static void Draw(PixelFrame frame, PixelRect bounds)
    {
        var cursor = new CursorInfo { Size = Marshal.SizeOf<CursorInfo>() };
        if (!GetCursorInfo(ref cursor) || (cursor.Flags & 1) == 0 || cursor.Cursor == 0) return;
        if (!GetPhysicalCursorPos(out var point)) return;
        Draw(frame, bounds, cursor.Cursor, point.X, point.Y);
    }

    internal static void Draw(PixelFrame frame, PixelRect bounds, nint cursor, int cursorX, int cursorY)
    {
        if (!GetIconInfo(cursor, out var icon)) return;
        nint dc = 0, dib = 0, previous = 0;
        try
        {
            nint shape = icon.Color != 0 ? icon.Color : icon.Mask;
            if (GetObjectW(shape, Marshal.SizeOf<BitmapInfo>(), out var bitmap) == 0) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot read cursor bitmap.");
            int width = bitmap.Width, height = icon.Color != 0 ? bitmap.Height : bitmap.Height / 2;
            int left = cursorX - bounds.X - (int)icon.HotspotX, top = cursorY - bounds.Y - (int)icon.HotspotY;
            var visible = new PixelRect(left, top, width, height).Intersect(new(0, 0, frame.Width, frame.Height));
            if (visible.IsEmpty) return;
            dc = CreateCompatibleDC(0);
            if (dc == 0) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot create cursor drawing surface.");
            var header = new DibHeader { Size = (uint)Marshal.SizeOf<DibHeader>(), Width = width, Height = -height, Planes = 1, Bits = 32 };
            dib = CreateDIBSection(dc, ref header, 0, out nint pixels, 0, 0);
            if (dib == 0) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot allocate cursor bitmap.");
            previous = SelectObject(dc, dib);
            for (int y = visible.Y; y < visible.Bottom; y++)
                Marshal.Copy(frame.Pixels, y * frame.Stride + visible.X * 4, pixels + ((y - top) * width + visible.X - left) * 4, visible.Width * 4);
            if (!DrawIconEx(dc, 0, 0, cursor, width, height, 0, 0, 3))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot draw the native cursor.");
            GdiFlush();
            for (int y = visible.Y; y < visible.Bottom; y++)
            {
                Marshal.Copy(pixels + ((y - top) * width + visible.X - left) * 4, frame.Pixels, y * frame.Stride + visible.X * 4, visible.Width * 4);
                for (int x = visible.X; x < visible.Right; x++) frame.Pixels[y * frame.Stride + x * 4 + 3] = 255;
            }
        }
        finally
        {
            if (previous != 0 && dc != 0) SelectObject(dc, previous);
            if (dib != 0) DeleteObject(dib);
            if (dc != 0) DeleteDC(dc);
            if (icon.Mask != 0) DeleteObject(icon.Mask);
            if (icon.Color != 0) DeleteObject(icon.Color);
        }
    }

    [StructLayout(LayoutKind.Sequential)] private struct Point { internal int X, Y; }
    [StructLayout(LayoutKind.Sequential)] private struct CursorInfo { internal int Size, Flags; internal nint Cursor; internal Point Position; }
    [StructLayout(LayoutKind.Sequential)] private struct IconInfo { internal int IsIcon; internal uint HotspotX, HotspotY; internal nint Mask, Color; }
    [StructLayout(LayoutKind.Sequential)] private struct BitmapInfo { internal int Type, Width, Height, Stride; internal ushort Planes, Bits; internal nint Pixels; }
    [StructLayout(LayoutKind.Sequential)] private struct DibHeader { internal uint Size; internal int Width, Height; internal ushort Planes, Bits; internal uint Compression, ImageSize; internal int XDensity, YDensity; internal uint Colors, Important; }
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetCursorInfo(ref CursorInfo info);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetPhysicalCursorPos(out Point point);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetIconInfo(nint icon, out IconInfo info);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool DrawIconEx(nint dc, int x, int y, nint icon, int width, int height, uint step, nint brush, uint flags);
    [DllImport("gdi32.dll", SetLastError = true)] private static extern int GetObjectW(nint item, int size, out BitmapInfo info);
    [DllImport("gdi32.dll", SetLastError = true)] private static extern nint CreateCompatibleDC(nint dc);
    [DllImport("gdi32.dll", SetLastError = true)] private static extern nint CreateDIBSection(nint dc, ref DibHeader info, uint usage, out nint bits, nint section, uint offset);
    [DllImport("gdi32.dll")] private static extern nint SelectObject(nint dc, nint item);
    [DllImport("gdi32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool DeleteObject(nint item);
    [DllImport("gdi32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool DeleteDC(nint dc);
    [DllImport("gdi32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GdiFlush();
}
