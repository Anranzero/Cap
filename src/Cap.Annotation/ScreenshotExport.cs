using System.IO;
using System.Windows.Media.Imaging;

namespace Cap.Annotation;

internal static class ScreenshotExport
{
    /// <summary>Encode next to the destination, then publish only the completed image.</summary>
    public static void Save(BitmapSource image, string destination)
    {
        ArgumentNullException.ThrowIfNull(image);
        ArgumentException.ThrowIfNullOrWhiteSpace(destination);
        string target = Path.GetFullPath(destination);
        string directory = Path.GetDirectoryName(target) ?? throw new ArgumentException("保存路径缺少目录。", nameof(destination));
        string temporary = Path.Combine(directory, ".cap-export-" + Guid.NewGuid().ToString("N") + ".tmp");
        bool ownsTemporary = false;
        Exception? exportError = null;
        try
        {
            string extension = Path.GetExtension(target).ToLowerInvariant();
            BitmapEncoder encoder = extension is ".jpg" or ".jpeg" ? new JpegBitmapEncoder { QualityLevel = 94 } : new PngBitmapEncoder();
            encoder.Frames.Add(BitmapFrame.Create(image));
            using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None, 4096, FileOptions.WriteThrough))
            {
                ownsTemporary = true;
                encoder.Save(stream);
                stream.Flush(true);
            }
            // The sibling temporary file is on the same volume; replacement happens after encoding and flush.
            File.Move(temporary, target, true);
            ownsTemporary = false;
        }
        catch (Exception ex)
        {
            exportError = ex;
            throw;
        }
        finally
        {
            if (ownsTemporary)
            {
                try { File.Delete(temporary); }
                catch (Exception cleanupError) when (cleanupError is IOException or UnauthorizedAccessException)
                {
                    if (exportError is null) throw;
                    exportError.Data["TemporaryFileCleanupError"] = cleanupError;
                    global::System.Diagnostics.Trace.TraceError("Could not clean screenshot export temporary file: {0}", cleanupError);
                }
            }
        }
    }
}
