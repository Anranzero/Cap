using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;
using Cap.Core;
using Cap.Recording;
using static Program;
using static EncoderTests;

internal static class OverlayTests
{
    internal static void Run()
    {
        GetCursorPos(out var original);
        using var form = new Form { Text = "Cap recording overlay test", Bounds = new Rectangle(120, 120, 420, 320), TopMost = true };
        try
        {
            form.Show();
            Application.DoEvents();
            SetCursorPos(300, 260);
            Application.DoEvents();
            SetCursor(Cursors.Arrow.Handle);
            GetPhysicalCursorPos(out var physical);
            var bounds = new PixelRect(physical.X - 64, physical.Y - 64, 128, 128);
            var options = new RecordingOptions { Bounds = bounds, OutputPath = "overlay.gif", Format = RecordingFormat.Gif,
                ShowCursor = false, ShowCoordinates = true, HighlightClicks = false, CoordinateColor = "#00FF00" };
            using (var processor = new FrameProcessor(options))
            {
                var frame = processor.Process(Solid(128, 128, 0, 0, 0), bounds, 128, 128);
                int green = 0;
                for (int p = 0; p < frame.Pixels.Length; p += 4)
                    if (frame.Pixels[p + 1] > 80 && frame.Pixels[p + 2] < 30 && frame.Pixels[p] < 30) green++;
                Assert(green > 20, "Configured green coordinate text must be present in output pixels.");
            }
            using (var processor = new FrameProcessor(options with { ShowCursor = true, ShowCoordinates = false }))
            {
                byte[] xor = new byte[128];
                Array.Fill(xor, (byte)255);
                nint custom = CreateCursor(0, 4, 6, 32, 32, new byte[128], xor);
                Assert(custom != 0, "Owned deterministic cursor must be created.");
                try
                {
                    var frame = Solid(128, 128, 128, 128, 128);
                    CursorRenderer.Draw(frame, bounds, custom, physical.X, physical.Y);
                    int visible = 0;
                    for (int p = 0; p < frame.Pixels.Length; p += 4) if (frame.Pixels[p] != 128 || frame.Pixels[p + 1] != 128 || frame.Pixels[p + 2] != 128) visible++;
                    Assert(visible >= 32 * 32, "The native cursor must be drawn into output pixels.");
                    Assert(frame.Pixels[((64 - 6) * 128 + 64 - 4) * 4] == 255, "Native cursor hotspot must align to the captured pointer position.");
                }
                finally { DestroyCursor(custom); }
            }
            using (var processor = new FrameProcessor(options with { ShowCoordinates = false, HighlightClicks = true }))
            {
                var frame = Solid(128, 128, 0, 0, 0);
                var pin = GCHandle.Alloc(frame.Pixels, GCHandleType.Pinned);
                try
                {
                    using var bitmap = new Bitmap(128, 128, frame.Stride, System.Drawing.Imaging.PixelFormat.Format32bppRgb, pin.AddrOfPinnedObject());
                    using var graphics = Graphics.FromImage(bitmap);
                    FrameProcessor.DrawClick(graphics, bounds, new MouseClick(physical.X, physical.Y, Environment.TickCount64 - 60));
                }
                finally { pin.Free(); }
                int yellow = 0;
                for (int p = 0; p < frame.Pixels.Length; p += 4)
                    if (frame.Pixels[p + 2] > 50 && frame.Pixels[p + 1] > 30 && frame.Pixels[p] < 20) yellow++;
                Assert(yellow > 20, "A recent click must leave a visible pulse in output pixels.");
            }
            using (var processor = new FrameProcessor(options with { ShowCoordinates = false }))
            {
                var frame = processor.Process(Solid(128, 128, 50, 100, 150), bounds, 64, 64);
                Assert(frame.Width == 64 && frame.Height == 64 && frame.Pixels.Length == 64 * 64 * 4, "Resize must emit exact tightly packed dimensions.");
                int center = (32 * 64 + 32) * 4;
                Assert(frame.Pixels[center] == 150 && frame.Pixels[center + 1] == 100 && frame.Pixels[center + 2] == 50 && frame.Pixels[center + 3] == 255, "Resize should preserve a solid color and opaque alpha.");
            }
        }
        finally { form.Close(); SetCursorPos(original.X, original.Y); }
    }
    [StructLayout(LayoutKind.Sequential)] private struct Point { internal int X, Y; }
    [DllImport("user32.dll")] private static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] private static extern bool GetPhysicalCursorPos(out Point point);
    [DllImport("user32.dll")] private static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] private static extern nint SetCursor(nint cursor);
    [DllImport("user32.dll")] private static extern nint CreateCursor(nint instance, int hotX, int hotY, int width, int height, byte[] andMask, byte[] xorMask);
    [DllImport("user32.dll")] private static extern bool DestroyCursor(nint cursor);
}
