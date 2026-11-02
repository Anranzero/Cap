using System.Globalization;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cap.Core;

namespace Cap.Annotation;

public enum AnnotationTool { Select, Rectangle, Ellipse, Arrow, Line, Pen, Text, Number, Pixelate, Blur, Marker }

internal sealed record PaintCommand(AnnotationTool Tool, Point Start, Point End, Color Color, double Thickness,
    IReadOnlyList<Point>? Points = null, string? Text = null, int Number = 0, BitmapSource? Stamp = null, double Opacity = 1, int TextStyle = 0)
{
    public Rect Bounds => new(Start, End);
    public long StorageBytes => Stamp is null ? (Points?.Count ?? 2) * 16L + (Text?.Length ?? 0) * 2L : (long)Stamp.PixelWidth * Stamp.PixelHeight * 4;

    public void Draw(DrawingContext dc)
    {
        Brush brush = new SolidColorBrush(Color);
        var pen = new Pen(brush, Thickness) { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round, LineJoin = PenLineJoin.Round };
        Rect rect = Bounds;
        switch (Tool)
        {
            case AnnotationTool.Rectangle: dc.DrawRectangle(null, pen, rect); break;
            case AnnotationTool.Ellipse: dc.DrawEllipse(null, pen, new Point(rect.X + rect.Width / 2, rect.Y + rect.Height / 2), rect.Width / 2, rect.Height / 2); break;
            case AnnotationTool.Line: dc.DrawLine(pen, Start, End); break;
            case AnnotationTool.Arrow:
                dc.DrawLine(pen, Start, End);
                Vector direction = End - Start;
                if (direction.Length < 1) break;
                direction.Normalize();
                Vector normal = new(-direction.Y, direction.X);
                double size = Math.Max(12, Thickness * 4);
                var arrow = new StreamGeometry();
                using (StreamGeometryContext path = arrow.Open())
                {
                    path.BeginFigure(End, true, true);
                    path.LineTo(End - direction * size + normal * size * .45, true, false);
                    path.LineTo(End - direction * size - normal * size * .45, true, false);
                }
                dc.DrawGeometry(brush, null, arrow);
                break;
            case AnnotationTool.Marker:
                if (Points is null || Points.Count == 0) break;
                var highlight = Color.FromArgb((byte)Math.Clamp((int)Math.Round(Opacity * 255), 8, 255), Color.R, Color.G, Color.B);
                var markerPen = new Pen(new SolidColorBrush(highlight), Math.Max(8, Thickness * 2.6))
                { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round, LineJoin = PenLineJoin.Round };
                var markerGeometry = new StreamGeometry();
                using (StreamGeometryContext markerPath = markerGeometry.Open())
                {
                    markerPath.BeginFigure(Points[0], false, false);
                    markerPath.PolyLineTo(Points.Skip(1).ToArray(), true, true);
                }
                dc.DrawGeometry(null, markerPen, markerGeometry);
                if (Points.Count == 1) dc.DrawEllipse(new SolidColorBrush(highlight), null, Start, Math.Max(4, Thickness * 1.3), Math.Max(4, Thickness * 1.3));
                break;
            case AnnotationTool.Pen:
                if (Points is null || Points.Count == 0) break;
                var geometry = new StreamGeometry();
                using (StreamGeometryContext path = geometry.Open())
                {
                    path.BeginFigure(Points[0], false, false);
                    path.PolyLineTo(Points.Skip(1).ToArray(), true, true);
                }
                dc.DrawGeometry(null, pen, geometry);
                if (Points.Count == 1) dc.DrawEllipse(brush, null, Start, Thickness / 2, Thickness / 2);
                break;
            case AnnotationTool.Text:
                double textSize = Math.Max(16, Thickness * 5);
                if (TextStyle == 1)
                {
                    // Feishu text style 2: outlined lettering (white core, coloured contour).
                    var outline = MakeText(Text ?? "", textSize, brush, serif: false);
                    Geometry outlineShape = outline.BuildGeometry(Start);
                    dc.DrawGeometry(Brushes.White, new Pen(brush, Math.Max(1.5, textSize * .08)), outlineShape);
                }
                else
                {
                    dc.DrawText(MakeText(Text ?? "", textSize, brush, TextStyle == 2), Start);
                }
                break;
            case AnnotationTool.Number:
                double radius = Math.Max(13, Thickness * 4);
                dc.DrawEllipse(brush, null, Start, radius, radius);
                var label = MakeText(Number.ToString(CultureInfo.InvariantCulture), radius * 1.25, Brushes.White);
                dc.DrawText(label, new Point(Start.X - label.Width / 2, Start.Y - label.Height / 2));
                break;
            case AnnotationTool.Pixelate:
            case AnnotationTool.Blur:
                if (Stamp is not null) dc.DrawImage(Stamp, rect);
                else dc.DrawRectangle(new SolidColorBrush(System.Windows.Media.Color.FromArgb(40, 255, 255, 255)), pen, rect);
                break;
        }
    }

    internal static FormattedText MakeText(string value, double size, Brush brush, bool serif = false) =>
        new(value, CultureInfo.CurrentUICulture, FlowDirection.LeftToRight, new Typeface(serif ? "SimSun" : "Microsoft YaHei UI"), size, brush, 1);
}

/// <summary>Annotations use physical pixels relative to the frozen virtual desktop.</summary>
internal sealed class AnnotationDocument
{
    private const long HistoryBudget = 128L * 1024 * 1024;
    private readonly List<PaintCommand> commands = [];
    private readonly Stack<PaintCommand> redo = [];
    private long historyBytes;
    public BitmapSource Background { get; }
    public bool CanUndo => commands.Count > 0;
    public bool CanRedo => redo.Count > 0;
    private int bakedNumbers;
    public int NextNumber => bakedNumbers + commands.Count(c => c.Tool == AnnotationTool.Number) + 1;

    public AnnotationDocument(PixelFrame frame)
    {
        frame.Validate();
        Background = BitmapSource.Create(frame.Width, frame.Height, 96, 96, PixelFormats.Bgra32, null, frame.Pixels, frame.Stride);
        Background.Freeze();
    }

    public void Add(PaintCommand command)
    {
        historyBytes -= redo.Sum(c => c.StorageBytes);
        redo.Clear();
        commands.Add(command);
        historyBytes += command.StorageBytes;
        // Preserve the picture while bounding undo memory: bake old operations into the base.
        while (commands.Count > 80 || historyBytes > HistoryBudget && commands.Count > 1)
        {
            PaintCommand first = commands[0];
            Bake(first);
            commands.RemoveAt(0);
            historyBytes -= first.StorageBytes;
        }
    }

    private BitmapSource? baked;
    private void Bake(PaintCommand command)
    {
        var visual = new DrawingVisual();
        using (DrawingContext dc = visual.RenderOpen())
        {
            dc.DrawImage(baked ?? Background, new Rect(0, 0, Background.PixelWidth, Background.PixelHeight));
            command.Draw(dc);
        }
        baked = Render(visual, Background.PixelWidth, Background.PixelHeight);
        if (command.Tool == AnnotationTool.Number) bakedNumbers++;
    }

    public void DrawCurrent(DrawingContext dc)
    {
        dc.DrawImage(baked ?? Background, new Rect(0, 0, Background.PixelWidth, Background.PixelHeight));
        foreach (PaintCommand command in commands) command.Draw(dc);
    }

    public void Undo() { if (commands.Count > 0) { redo.Push(commands[^1]); commands.RemoveAt(commands.Count - 1); } }
    public void Redo() { if (redo.Count > 0) commands.Add(redo.Pop()); }

    public BitmapSource Export(PixelRect selection)
    {
        if (selection.IsEmpty) throw new ArgumentException("选区为空。", nameof(selection));
        var visual = new DrawingVisual();
        using (DrawingContext dc = visual.RenderOpen())
        {
            dc.PushTransform(new TranslateTransform(-selection.X, -selection.Y));
            DrawCurrent(dc);
            dc.Pop();
        }
        return Render(visual, selection.Width, selection.Height);
    }

    private static BitmapSource Render(Visual visual, int width, int height)
    {
        var bitmap = new RenderTargetBitmap(width, height, 96, 96, PixelFormats.Pbgra32);
        bitmap.Render(visual);
        bitmap.Freeze();
        return bitmap;
    }

    public PaintCommand Effect(AnnotationTool tool, PixelRect rect, Color color, double thickness)
    {
        BitmapSource bitmap = Export(rect);
        byte[] data = new byte[checked(rect.Width * rect.Height * 4)];
        bitmap.CopyPixels(data, rect.Width * 4, 0);
        if (tool == AnnotationTool.Pixelate) RasterEffects.Pixelate(data, rect.Width, rect.Height, Math.Max(8, (int)thickness * 4));
        else RasterEffects.Blur(data, rect.Width, rect.Height, Math.Max(3, (int)thickness * 2));
        var stamp = BitmapSource.Create(rect.Width, rect.Height, 96, 96, PixelFormats.Pbgra32, null, data, rect.Width * 4);
        stamp.Freeze();
        return new PaintCommand(tool, new Point(rect.X, rect.Y), new Point(rect.Right, rect.Bottom), color, thickness, Stamp: stamp);
    }
}

internal static class RasterEffects
{
    public static void Pixelate(byte[] pixels, int width, int height, int block)
    {
        for (int y = 0; y < height; y += block)
        for (int x = 0; x < width; x += block)
        {
            int right = Math.Min(width, x + block), bottom = Math.Min(height, y + block);
            long b = 0, g = 0, r = 0, a = 0;
            int count = (right - x) * (bottom - y);
            for (int yy = y; yy < bottom; yy++)
            for (int xx = x; xx < right; xx++)
            {
                int i = (yy * width + xx) * 4;
                b += pixels[i]; g += pixels[i + 1]; r += pixels[i + 2]; a += pixels[i + 3];
            }
            for (int yy = y; yy < bottom; yy++)
            for (int xx = x; xx < right; xx++)
            {
                int i = (yy * width + xx) * 4;
                pixels[i] = (byte)(b / count); pixels[i + 1] = (byte)(g / count);
                pixels[i + 2] = (byte)(r / count); pixels[i + 3] = (byte)(a / count);
            }
        }
    }

    public static void Blur(byte[] pixels, int width, int height, int radius)
    {
        byte[] horizontal = new byte[pixels.Length];
        // Two sliding-window passes, linear in the number of pixels.
        for (int y = 0; y < height; y++)
        for (int channel = 0; channel < 4; channel++)
        {
            long sum = 0;
            for (int x = 0; x <= Math.Min(radius, width - 1); x++) sum += pixels[(y * width + x) * 4 + channel];
            for (int x = 0; x < width; x++)
            {
                int left = Math.Max(0, x - radius), right = Math.Min(width - 1, x + radius);
                horizontal[(y * width + x) * 4 + channel] = (byte)(sum / (right - left + 1));
                if (x - radius >= 0) sum -= pixels[(y * width + x - radius) * 4 + channel];
                if (x + radius + 1 < width) sum += pixels[(y * width + x + radius + 1) * 4 + channel];
            }
        }
        for (int x = 0; x < width; x++)
        for (int channel = 0; channel < 4; channel++)
        {
            long sum = 0;
            for (int y = 0; y <= Math.Min(radius, height - 1); y++) sum += horizontal[(y * width + x) * 4 + channel];
            for (int y = 0; y < height; y++)
            {
                int top = Math.Max(0, y - radius), bottom = Math.Min(height - 1, y + radius);
                pixels[(y * width + x) * 4 + channel] = (byte)(sum / (bottom - top + 1));
                if (y - radius >= 0) sum -= horizontal[((y - radius) * width + x) * 4 + channel];
                if (y + radius + 1 < height) sum += horizontal[((y + radius + 1) * width + x) * 4 + channel];
            }
        }
    }
}
