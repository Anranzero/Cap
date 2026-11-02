using System.Globalization;
using System.Reflection;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Windows.Media.Ocr;

internal static class Program
{
    [STAThread]
    private static int Main()
    {
        try
        {
            RunAsync().GetAwaiter().GetResult();
            return 0;
        }
        catch (Exception ex) { Console.Error.WriteLine(ex); return 1; }
    }

    private static async Task RunAsync()
    {
        Type service = typeof(Cap.Annotation.ScreenshotWorkflow).Assembly.GetType("Cap.Annotation.OcrService")
            ?? throw new InvalidOperationException("FAIL: real OCR service entry point is missing.");
        MethodInfo extract = service.GetMethod("ExtractTextAsync", BindingFlags.Public | BindingFlags.Static)
            ?? throw new InvalidOperationException("FAIL: ExtractTextAsync entry point is missing.");
        Task<string> Extract(BitmapSource image, CancellationToken token = default)
        {
            try { return (Task<string>)extract.Invoke(null, [image, token])!; }
            catch (TargetInvocationException ex) when (ex.InnerException != null) { throw ex.InnerException; }
        }

        await ExpectAsync<ArgumentNullException>(() => Extract(null!), "null image validation");
        var canceled = new CancellationToken(true);
        await ExpectAsync<OperationCanceledException>(() => Extract(RenderText(800, 180, 44), canceled), "pre-canceled OCR");
        Console.WriteLine("PASS: null image validation and pre-canceled OCR.");

        string[] languages = OcrEngine.AvailableRecognizerLanguages.Select(language => language.LanguageTag).ToArray();
        Console.WriteLine("OCR model inventory: " + (languages.Length == 0 ? "none" : string.Join(", ", languages)));
        if (languages.Length == 0)
        {
            try { await Extract(RenderText(800, 180, 44)); }
            catch (InvalidOperationException ex) when (ex.Message.Contains("语言") && ex.Message.Contains("Windows"))
            {
                Console.WriteLine("PASS: missing models produce explicit Windows language installation guidance.");
                Console.WriteLine("SKIP: real text recognition, tiny-image and oversized-image OCR; no installed Windows OCR model.");
                return;
            }
            throw new InvalidOperationException("FAIL: missing OCR models must not return fake or empty recognition success.");
        }

        BitmapSource image = RenderText(800, 180, 44);
        string actual = await Extract(image);
        CheckText(actual, "standard screenshot");
        Console.WriteLine("PASS: real offline recognition: " + actual.ReplaceLineEndings(" | "));

        var upscaled = new TransformedBitmap(image, new ScaleTransform(1.5, 1.5));
        upscaled.Freeze();
        CheckText(await Extract(upscaled), "upscaled screenshot source");
        Console.WriteLine("PASS: upscaled BitmapSource is recognized.");

        string multiline = await Extract(RenderText(800, 260, 44, "CAP SCREENSHOT 12345\nSECOND LINE 67890"));
        Check(multiline.Contains(Environment.NewLine, StringComparison.Ordinal) && multiline.Contains("67890", StringComparison.Ordinal),
            "Multiline recognition must preserve line breaks and the second line.");
        Console.WriteLine("PASS: recognized lines retain line breaks.");

        BitmapSource small = BitmapSource.Create(1, 1, 96, 96, PixelFormats.Bgra32, null, new byte[] { 255, 255, 255, 255 }, 4);
        small.Freeze();
        Check(string.IsNullOrWhiteSpace(await Extract(small)), "Tiny white images must produce no invented text.");
        Console.WriteLine("PASS: one-pixel input is accepted without invented text.");

        byte[] hiddenPixels = new byte[image.PixelWidth * image.PixelHeight * 4];
        new FormatConvertedBitmap(image, PixelFormats.Bgra32, null, 0).CopyPixels(hiddenPixels, image.PixelWidth * 4, 0);
        for (int i = 3; i < hiddenPixels.Length; i += 4) hiddenPixels[i] = 0;
        BitmapSource transparent = BitmapSource.Create(image.PixelWidth, image.PixelHeight, 96, 96, PixelFormats.Bgra32, null,
            hiddenPixels, image.PixelWidth * 4);
        transparent.Freeze();
        Check(string.IsNullOrWhiteSpace(await Extract(transparent)), "Transparent hidden RGB text must not become visible to OCR.");
        Console.WriteLine("PASS: transparent RGB text is composited onto white, not recognized as visible text.");

        int wide = checked((int)OcrEngine.MaxImageDimension + 256);
        CheckText(await Extract(RenderText(wide, 220, 64)), "oversized screenshot scaled to the OCR limit");
        Console.WriteLine("PASS: oversized source is scaled and remains recognizable.");

        var crop = new CroppedBitmap(RenderText(800, 360, 44, "CAP SCREENSHOT 12345\nOUTSIDE CROP 98765"), new Int32Rect(0, 0, 800, 95));
        crop.Freeze();
        string cropped = await Extract(crop);
        CheckText(cropped, "cropped screenshot");
        Check(!cropped.Contains("98765", StringComparison.Ordinal), "OCR must only read the cropped image.");
        Console.WriteLine("PASS: crop excludes text outside the supplied bitmap.");
    }

    private static BitmapSource RenderText(int width, int height, double fontSize, string text = "CAP SCREENSHOT 12345")
    {
        var visual = new DrawingVisual();
        using (DrawingContext context = visual.RenderOpen())
        {
            context.DrawRectangle(Brushes.White, null, new Rect(0, 0, width, height));
            var formatted = new FormattedText(text, CultureInfo.GetCultureInfo("en-US"), FlowDirection.LeftToRight,
                new Typeface("Arial"), fontSize, Brushes.Black, 1);
            context.DrawText(formatted, new Point(28, 28));
        }
        var bitmap = new RenderTargetBitmap(width, height, 96, 96, PixelFormats.Pbgra32);
        bitmap.Render(visual); bitmap.Freeze(); return bitmap;
    }

    private static void CheckText(string text, string scenario)
    {
        string normalized = string.Join(" ", text.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries)).ToUpperInvariant();
        Check(normalized.Contains("CAP SCREENSHOT 12345", StringComparison.Ordinal), $"{scenario}: actual text was '{text}'.");
    }

    private static async Task ExpectAsync<T>(Func<Task<string>> action, string scenario) where T : Exception
    {
        try { await action(); }
        catch (T) { return; }
        throw new InvalidOperationException("FAIL: " + scenario + " did not throw " + typeof(T).Name);
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException("FAIL: " + message);
    }
}
