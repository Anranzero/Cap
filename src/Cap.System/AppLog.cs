using System;
using System.IO;

namespace Cap.Platform;

public static class AppLog
{
    private static readonly object Gate = new();
    public static string DirectoryPath => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "cap", "logs");
    public static void Error(Exception exception)
    {
        try
        {
            lock (Gate)
            {
                Directory.CreateDirectory(DirectoryPath);
                string file = Path.Combine(DirectoryPath, "cap.log");
                if (File.Exists(file) && new FileInfo(file).Length > 2 * 1024 * 1024) File.Move(file, file + ".previous", true);
                File.AppendAllText(file, DateTimeOffset.Now.ToString("O") + " " + exception + Environment.NewLine);
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        { global::System.Diagnostics.Trace.WriteLine("cap log unavailable: " + ex.Message); }
    }
}
