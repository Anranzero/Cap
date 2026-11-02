using System;
using System.IO;
using System.Text.Json;

namespace Cap.Platform;

public sealed class SettingsStore
{
    private readonly string directory;
    public string FilePath => Path.Combine(directory, "settings.json");
    public string? LoadWarning { get; private set; }
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    public SettingsStore(string? directory = null) => this.directory = directory ??
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "cap");

    public AppSettings Load()
    {
        LoadWarning = null;
        if (!File.Exists(FilePath)) return new();
        try
        {
            var settings = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(FilePath), JsonOptions)
                ?? throw new JsonException("配置为空。");
            settings.Validate();
            return settings;
        }
        catch (Exception ex) when (ex is JsonException or ArgumentException or IOException or UnauthorizedAccessException)
        {
            LoadWarning = "配置读取失败，已使用默认设置。原配置仍保留。" + ex.Message;
            return new();
        }
    }

    public void Save(AppSettings settings)
    {
        settings.Validate();
        Directory.CreateDirectory(directory);
        string temp = Path.Combine(directory, "settings." + Guid.NewGuid().ToString("N") + ".tmp");
        try
        {
            using (var stream = new FileStream(temp, FileMode.CreateNew, FileAccess.Write, FileShare.None, 4096, FileOptions.WriteThrough))
            {
                JsonSerializer.Serialize(stream, settings, JsonOptions);
                stream.Flush(true);
            }
            if (File.Exists(FilePath)) BackupExistingSettings();
            File.Move(temp, FilePath, true);
        }
        finally
        {
            try { if (File.Exists(temp)) File.Delete(temp); }
            catch (Exception cleanup) when (cleanup is IOException or UnauthorizedAccessException)
            { AppLog.Error(new IOException("配置临时文件清理失败：" + temp, cleanup)); }
        }
    }

    private void BackupExistingSettings()
    {
        string backup = FilePath + ".bak";
        bool valid = true;
        try
        {
            var previous = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(FilePath), JsonOptions)
                ?? throw new JsonException("配置为空。");
            previous.Validate();
        }
        catch (Exception ex) when (ex is JsonException or ArgumentException)
        {
            valid = false;
            backup = Path.Combine(directory, "settings.corrupt." + DateTime.UtcNow.ToString("yyyyMMdd-HHmmss-fff") + "." + Guid.NewGuid().ToString("N") + ".json");
        }
        File.Copy(FilePath, backup, valid);
    }
}
