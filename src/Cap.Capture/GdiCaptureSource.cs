using System.ComponentModel;
using System.Runtime.InteropServices;
using Cap.Core;

namespace Cap.Capture;

internal sealed class GdiCaptureSource : ICaptureSource
{
    private readonly object gate = new();
    private bool disposed;
    public PixelRect Bounds { get; }
    public string BackendName => "GDI";

    internal GdiCaptureSource(PixelRect bounds)
    {
        CaptureService.ValidateBounds(bounds);
        Bounds = bounds;
    }

    public unsafe PixelFrame Capture()
    {
        lock (gate)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            using var dpi = new NativeMethods.DpiScope();
            nint desktop = NativeMethods.GetDC(0);
            if (desktop == 0) throw NativeFailure("Cannot acquire the desktop device context.");
            nint memory = 0, bitmap = 0, previous = 0;
            try
            {
                memory = NativeMethods.CreateCompatibleDC(desktop);
                if (memory == 0) throw NativeFailure("Cannot create a capture device context.");
                var info = new NativeMethods.BitmapInfo
                {
                    Size = (uint)Marshal.SizeOf<NativeMethods.BitmapInfo>(),
                    Width = Bounds.Width,
                    Height = -Bounds.Height,
                    Planes = 1,
                    BitCount = 32,
                    SizeImage = (uint)checked(Bounds.Width * Bounds.Height * 4)
                };
                bitmap = NativeMethods.CreateDIBSection(desktop, ref info, 0, out nint pixels, 0, 0);
                if (bitmap == 0 || pixels == 0) throw NativeFailure("Cannot allocate screenshot pixels.");
                previous = NativeMethods.SelectObject(memory, bitmap);
                if (previous == 0 || previous == NativeMethods.HgdiError) throw NativeFailure("Cannot select the screenshot bitmap.");
                int length = checked(Bounds.Width * Bounds.Height * 4);
                new Span<byte>((void*)pixels, length).Clear();
                PixelRect visible = Bounds.Intersect(CaptureService.GetVirtualBounds());
                if (!visible.IsEmpty && !NativeMethods.BitBlt(memory, visible.X - Bounds.X, visible.Y - Bounds.Y,
                        visible.Width, visible.Height, desktop, visible.X, visible.Y, NativeMethods.Srccopy | NativeMethods.CaptureBlt))
                    throw NativeFailure("Desktop capture failed.");
                if (!NativeMethods.GdiFlush()) throw NativeFailure("Cannot flush screenshot drawing operations.");
                byte[] buffer = GC.AllocateUninitializedArray<byte>(length);
                Marshal.Copy(pixels, buffer, 0, length);
                for (int alpha = 3; alpha < length; alpha += 4) buffer[alpha] = 255;
                return new(Bounds.Width, Bounds.Height, buffer);
            }
            finally
            {
                if (previous != 0 && previous != NativeMethods.HgdiError) NativeMethods.SelectObject(memory, previous);
                if (bitmap != 0) NativeMethods.DeleteObject(bitmap);
                if (memory != 0) NativeMethods.DeleteDC(memory);
                NativeMethods.ReleaseDC(0, desktop);
            }
        }
    }

    private static Win32Exception NativeFailure(string message) => new(Marshal.GetLastWin32Error(), message);
    public void Dispose() { lock (gate) disposed = true; }
}
