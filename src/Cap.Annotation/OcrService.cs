using System.Windows.Media;
using System.Windows.Media.Imaging;
using Windows.Graphics.Imaging;
using Windows.Media.Ocr;
using Windows.Storage.Streams;

namespace Cap.Annotation;

/// <summary>Recognizes the supplied screenshot with installed Windows OCR models, entirely offline.</summary>
public static class OcrService
{
    public static async Task<string> ExtractTextAsync(BitmapSource image, CancellationToken token = default)
    {
        ArgumentNullException.ThrowIfNull(image);
        if (image.PixelWidth <= 0 || image.PixelHeight <= 0)
            throw new ArgumentException("请选择包含像素的截图后再提取文字。", nameof(image));
        token.ThrowIfCancellationRequested();
        OcrEngine engine = CreateEngine();
        BitmapSource source = image;
        double scale = Math.Min(1, OcrEngine.MaxImageDimension / (double)Math.Max(image.PixelWidth, image.PixelHeight));
        if (scale < 1)
            source = new TransformedBitmap(image, new ScaleTransform(scale, scale));
        if (source.Format != PixelFormats.Bgra32)
            source = new FormatConvertedBitmap(source, PixelFormats.Bgra32, null, 0);

        int stride = checked(source.PixelWidth * 4);
        byte[] pixels = new byte[checked(stride * source.PixelHeight)];
        source.CopyPixels(pixels, stride, 0);
        // Composite transparency onto white rather than exposing hidden RGB as visible text.
        for (int i = 0; i < pixels.Length; i += 4)
        {
            int alpha = pixels[i + 3];
            for (int channel = 0; channel < 3; channel++)
                pixels[i + channel] = (byte)((pixels[i + channel] * alpha + 255 * (255 - alpha) + 127) / 255);
            pixels[i + 3] = 255;
        }
        token.ThrowIfCancellationRequested();
        using var writer = new DataWriter();
        writer.WriteBytes(pixels);
        using SoftwareBitmap bitmap = SoftwareBitmap.CreateCopyFromBuffer(writer.DetachBuffer(), BitmapPixelFormat.Bgra8,
            source.PixelWidth, source.PixelHeight, BitmapAlphaMode.Ignore);
        // Keep the bitmap alive until native recognition finishes, including when canceled.
        // Early cancellation of the await would allow disposal while Windows still reads it.
        OcrResult result;
        try { result = await engine.RecognizeAsync(bitmap).AsTask().ConfigureAwait(false); }
        catch (Exception ex)
        {
            token.ThrowIfCancellationRequested();
            throw new InvalidOperationException("Windows 文字识别失败，请确认截图清晰且系统 OCR 语言包可用。", ex);
        }
        token.ThrowIfCancellationRequested();
        return string.Join(Environment.NewLine, result.Lines.Select(line => line.Text));
    }

    private static OcrEngine CreateEngine()
    {
        OcrEngine? engine = OcrEngine.TryCreateFromUserProfileLanguages();
        if (engine != null) return engine;
        var languages = OcrEngine.AvailableRecognizerLanguages
            .OrderBy(language => language.LanguageTag.StartsWith("zh", StringComparison.OrdinalIgnoreCase) ? 0
                : language.LanguageTag.StartsWith("en", StringComparison.OrdinalIgnoreCase) ? 1 : 2);
        foreach (var language in languages)
        {
            engine = OcrEngine.TryCreateFromLanguage(language);
            if (engine != null) return engine;
        }
        throw new InvalidOperationException("未找到可用的 Windows OCR 识别语言。请在 Windows 设置的语言选项中安装中文或英文 OCR 语言包，然后重试。cap 不会自动下载语言模型。");
    }
}
