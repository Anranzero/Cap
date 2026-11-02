using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.IO;
using Cap.Annotation;
using Cap.Core;

internal static class Program
{
    [STAThread]
    private static int Main()
    {
        try
        {
            SelectionGeometryTests();
            RasterEffectsTests();
            AnnotationHistoryTests();
            EveryToolRenders();
            ScreenshotExportTests();
            Console.WriteLine("PASS: selection geometry, raster pixelate/blur, crop resolution, annotation undo/redo/baking, branch history, every annotation tool, atomic screenshot export and locked-file preservation.");
            return 0;
        }
        catch (Exception ex) { Console.Error.WriteLine(ex); return 1; }
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void SelectionGeometryTests()
    {
        Check(SelectionGeometry.Drag(new Point(80, 70), new Point(-20, -10), 100, 100) == new PixelRect(0, 0, 80, 70), "Reverse drag must clamp to desktop.");
        PixelRect selection = new(20, 30, 40, 50);
        Check(SelectionGeometry.Adjust(selection, new Point(), new Point(200, 200), -1, 100, 100) == new PixelRect(60, 50, 40, 50), "Move must clamp without shrinking.");
        for (int handle = 0; handle < 8; handle++)
        {
            Point point = SelectionGeometry.Handles(selection)[handle];
            Check(SelectionGeometry.Hit(selection, point, 2) == handle, $"Handle {handle} must be hittable.");
            PixelRect resized = SelectionGeometry.Adjust(selection, point, new Point(200, -200), handle, 100, 100);
            Check(!resized.IsEmpty && resized.X >= 0 && resized.Y >= 0 && resized.Right <= 100 && resized.Bottom <= 100, $"Handle {handle} must preserve valid bounds.");
        }
        Check(new PixelRect(-1920, -100, 1920, 1080).Contains(-100, 0), "Negative desktop coordinates must remain valid.");
    }

    private static void RasterEffectsTests()
    {
        byte[] sample = [0, 20, 40, 255, 100, 120, 140, 255, 200, 220, 240, 255];
        byte[] mosaic = sample.ToArray();
        RasterEffects.Pixelate(mosaic, 3, 1, 2);
        Check(mosaic[0] == 50 && mosaic[4] == 50 && mosaic[8] == 200, "Pixelate must average blocks and preserve partial edge blocks.");
        byte[] blur = sample.ToArray();
        RasterEffects.Blur(blur, 3, 1, 1);
        Check(blur[0] == 50 && blur[4] == 100 && blur[8] == 150, "Blur must average neighbors with clamped edge windows.");
        Check(blur[3] == 255 && blur[7] == 255 && blur[11] == 255, "Blur must preserve opaque alpha.");
        byte[] single = [10, 20, 30, 255];
        RasterEffects.Blur(single, 1, 1, 30);
        Check(single.SequenceEqual(new byte[] { 10, 20, 30, 255 }), "Single-pixel blur must be stable.");
    }

    private static byte[] Pixels(BitmapSource bitmap)
    {
        byte[] pixels = new byte[bitmap.PixelWidth * bitmap.PixelHeight * 4];
        bitmap.CopyPixels(pixels, bitmap.PixelWidth * 4, 0);
        return pixels;
    }

    private static AnnotationDocument Document(int width = 32, int height = 24)
    {
        byte[] background = new byte[width * height * 4];
        for (int i = 0; i < background.Length; i += 4) { background[i] = 255; background[i + 3] = 255; }
        return new AnnotationDocument(new PixelFrame(width, height, background));
    }

    private static void AnnotationHistoryTests()
    {
        AnnotationDocument doc = Document();
        PixelRect crop = new(4, 3, 16, 12);
        BitmapSource image = doc.Export(crop);
        Check(image.PixelWidth == 16 && image.PixelHeight == 12 && image.DpiX == 96 && image.IsFrozen, "Export must use exact physical pixels with a frozen 96 DPI image.");
        byte[] before = Pixels(image);
        var line = new PaintCommand(AnnotationTool.Line, new Point(5, 6), new Point(17, 6), Colors.Red, 3);
        doc.Add(line);
        byte[] drawn = Pixels(doc.Export(crop));
        Check(!drawn.SequenceEqual(before), "Annotation must change exported pixels.");
        doc.Undo();
        Check(Pixels(doc.Export(crop)).SequenceEqual(before) && doc.CanRedo, "Undo must restore all exact pixels.");
        doc.Redo();
        Check(Pixels(doc.Export(crop)).SequenceEqual(drawn), "Redo must reproduce the edited image.");
        doc.Undo();
        doc.Add(line with { Color = Colors.Green });
        Check(!doc.CanRedo, "New edits must discard redo history.");
        for (int i = 0; i < 85; i++) doc.Add(line);
        byte[] baked = Pixels(doc.Export(crop));
        int undoCount = 0;
        while (doc.CanUndo) { doc.Undo(); undoCount++; }
        Check(undoCount == 80, "Undo history must be bounded to 80 operations.");
        Check(!Pixels(doc.Export(crop)).SequenceEqual(before), "Evicted edits must remain baked into the picture.");
        while (doc.CanRedo) doc.Redo();
        Check(Pixels(doc.Export(crop)).SequenceEqual(baked), "Undo/redo must preserve the baked base.");
        PaintCommand effect = doc.Effect(AnnotationTool.Pixelate, crop, Colors.Red, 3);
        doc.Add(effect);
        Check(effect.Stamp is { PixelWidth: 16, PixelHeight: 12 }, "Raster effects must retain exact crop resolution.");
    }

    private static void EveryToolRenders()
    {
        foreach (AnnotationTool tool in new[] { AnnotationTool.Rectangle, AnnotationTool.Ellipse, AnnotationTool.Arrow, AnnotationTool.Line, AnnotationTool.Pen, AnnotationTool.Text, AnnotationTool.Number })
        {
            AnnotationDocument doc = Document(160, 100);
            PixelRect area = new(0, 0, 160, 100);
            byte[] before = Pixels(doc.Export(area));
            var command = new PaintCommand(tool, new Point(30, 30), new Point(110, 70), Colors.Red, 3,
                [new Point(30, 30), new Point(55, 65), new Point(90, 35)], "文字\nCap", 1);
            doc.Add(command);
            Check(!Pixels(doc.Export(area)).SequenceEqual(before), $"{tool} must render actual pixels.");
            doc.Undo();
            Check(Pixels(doc.Export(area)).SequenceEqual(before), $"{tool} undo must be lossless.");
        }
        AnnotationDocument source = Document();
        source.Add(new PaintCommand(AnnotationTool.Line, new Point(0, 10), new Point(30, 10), Colors.Red, 2));
        PixelRect effectArea = new(0, 0, 32, 24);
        foreach (AnnotationTool tool in new[] { AnnotationTool.Pixelate, AnnotationTool.Blur })
        {
            byte[] before = Pixels(source.Export(effectArea));
            source.Add(source.Effect(tool, effectArea, Colors.Red, 3));
            Check(!Pixels(source.Export(effectArea)).SequenceEqual(before), $"{tool} must transform existing annotations as well as the background.");
            source.Undo();
            Check(Pixels(source.Export(effectArea)).SequenceEqual(before), $"{tool} undo must be lossless.");
        }
    }

    private static void ScreenshotExportTests()
    {
        string directory = Path.Combine(Path.GetTempPath(), "cap-export-tests-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            BitmapSource image = Document().Export(new PixelRect(0, 0, 32, 24));
            string png = Path.Combine(directory, "existing.png");
            byte[] original = [11, 22, 33, 44, 55];
            File.WriteAllBytes(png, original);
            ScreenshotExport.Save(image, png);
            using (var stream = File.OpenRead(png))
            {
                BitmapDecoder decoded = BitmapDecoder.Create(stream, BitmapCreateOptions.PreservePixelFormat, BitmapCacheOption.OnLoad);
                Check(decoded is PngBitmapDecoder && decoded.Frames[0].PixelWidth == 32 && decoded.Frames[0].PixelHeight == 24,
                    "Replacement must publish a complete PNG with the exact dimensions.");
            }
            string jpeg = Path.Combine(directory, "new.jpg");
            ScreenshotExport.Save(image, jpeg);
            using (var stream = File.OpenRead(jpeg))
            {
                BitmapDecoder decoded = BitmapDecoder.Create(stream, BitmapCreateOptions.PreservePixelFormat, BitmapCacheOption.OnLoad);
                Check(decoded is JpegBitmapDecoder && decoded.Frames[0].PixelWidth == 32 && decoded.Frames[0].PixelHeight == 24,
                    "Export must publish a complete JPEG with the exact dimensions.");
            }
            File.WriteAllBytes(png, original);
            using (var held = new FileStream(png, FileMode.Open, FileAccess.Read, FileShare.Read))
            {
                bool failed = false;
                try { ScreenshotExport.Save(image, png); }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { failed = true; }
                Check(failed, "Replacing a locked destination must report failure.");
                Check(File.ReadAllBytes(png).SequenceEqual(original), "A failed replacement must preserve all original bytes.");
                Check(!Directory.EnumerateFiles(directory, ".cap-export-*.tmp").Any(), "A failed replacement must clean its own temporary file.");
            }
            Check(Directory.EnumerateFiles(directory).Count() == 2, "Successful and failed exports must not leave temporary files.");
        }
        finally { Directory.Delete(directory, true); }
    }
}
