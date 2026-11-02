using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Globalization;
using System.Runtime.InteropServices;
using Cap.Core;

namespace Cap.Recording;

internal sealed class FrameProcessor : IDisposable
{
    private readonly RecordingOptions options;
    private readonly Font font;
    private readonly Brush textBrush;
    private readonly MouseClicks? clicks;

    internal FrameProcessor(RecordingOptions options)
    {
        this.options = options;
        font = new Font("Segoe UI", options.CoordinateFontSize, FontStyle.Regular, GraphicsUnit.Pixel);
        textBrush = new SolidBrush(ColorTranslator.FromHtml(options.CoordinateColor));
        try { if (options.HighlightClicks) clicks = new MouseClicks(); }
        catch { textBrush.Dispose(); font.Dispose(); throw; }
    }

    internal PixelFrame Process(PixelFrame frame, PixelRect bounds, int outputWidth, int outputHeight)
    {
        frame.Validate();
        if (options.ShowCursor) CursorRenderer.Draw(frame, bounds);
        var pin = GCHandle.Alloc(frame.Pixels, GCHandleType.Pinned);
        try
        {
            using var bitmap = new Bitmap(frame.Width, frame.Height, frame.Stride, PixelFormat.Format32bppRgb, pin.AddrOfPinnedObject());
            using (var graphics = Graphics.FromImage(bitmap))
            {
                graphics.SmoothingMode = SmoothingMode.AntiAlias;
                if (options.HighlightClicks && clicks is not null) DrawClick(graphics, bounds, clicks.Snapshot);
                if (options.ShowCoordinates && Native.GetPhysicalCursorPos(out var point))
                {
                    int x = point.X - (options.RelativeCoordinates ? bounds.X : 0);
                    int y = point.Y - (options.RelativeCoordinates ? bounds.Y : 0);
                    string text = string.Create(CultureInfo.InvariantCulture, $"X: {x}  Y: {y}");
                    SizeF measured = graphics.MeasureString(text, font);
                    float localX = Math.Clamp(point.X - bounds.X + 20f, 4, Math.Max(4, frame.Width - measured.Width - 4));
                    float localY = Math.Clamp(point.Y - bounds.Y + 24f, 4, Math.Max(4, frame.Height - measured.Height - 4));
                    using var backing = new SolidBrush(Color.FromArgb(185, 20, 20, 20));
                    graphics.FillRectangle(backing, localX - 3, localY - 2, measured.Width + 6, measured.Height + 4);
                    graphics.DrawString(text, font, textBrush, localX, localY);
                }
            }
            if (frame.Width == outputWidth && frame.Height == outputHeight) return frame;
            using var resized = new Bitmap(outputWidth, outputHeight, PixelFormat.Format32bppRgb);
            using (var graphics = Graphics.FromImage(resized))
            {
                graphics.CompositingMode = CompositingMode.SourceCopy;
                graphics.InterpolationMode = InterpolationMode.HighQualityBilinear;
                graphics.PixelOffsetMode = PixelOffsetMode.HighQuality;
                graphics.DrawImage(bitmap, new Rectangle(0, 0, outputWidth, outputHeight));
            }
            var data = resized.LockBits(new Rectangle(0, 0, outputWidth, outputHeight), ImageLockMode.ReadOnly, PixelFormat.Format32bppRgb);
            try
            {
                byte[] pixels = new byte[checked(outputWidth * outputHeight * 4)];
                for (int y = 0; y < outputHeight; y++)
                    Marshal.Copy(data.Scan0 + y * data.Stride, pixels, y * outputWidth * 4, outputWidth * 4);
                for (int p = 3; p < pixels.Length; p += 4) pixels[p] = 255;
                return new PixelFrame(outputWidth, outputHeight, pixels);
            }
            finally { resized.UnlockBits(data); }
        }
        finally { pin.Free(); }
    }

    internal static void DrawClick(Graphics graphics, PixelRect bounds, MouseClick click)
    {
        double age = (Environment.TickCount64 - click.Time) / 1000d;
        if (click.Time == 0 || age is < 0 or > 0.5) return;
        float radius = (float)(16 + age * 38);
        int alpha = (int)(220 * (1 - age / 0.5));
        using var brush = new SolidBrush(Color.FromArgb(alpha / 3, 255, 185, 0));
        using var pen = new Pen(Color.FromArgb(alpha, 255, 185, 0), 3);
        float x = click.X - bounds.X - radius, y = click.Y - bounds.Y - radius;
        graphics.FillEllipse(brush, x, y, radius * 2, radius * 2);
        graphics.DrawEllipse(pen, x, y, radius * 2, radius * 2);
    }

    public void Dispose() { clicks?.Dispose(); textBrush.Dispose(); font.Dispose(); }

    private static class Native
    {
        [StructLayout(LayoutKind.Sequential)] internal struct Point { internal int X, Y; }
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] internal static extern bool GetPhysicalCursorPos(out Point point);
    }
}
