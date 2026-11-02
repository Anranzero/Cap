using System;
using System.IO;
using Microsoft.Win32;

namespace Cap.Platform;

public static class StartupService
{
    private const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    public static void SetEnabled(bool enabled)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKey, true);
        if (enabled)
        {
            string path = Environment.ProcessPath ?? throw new InvalidOperationException("无法确定程序路径。");
            if (!string.Equals(Path.GetFileNameWithoutExtension(path), "cap", StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("请从发布后的 cap.exe 启用开机自启动。");
            key.SetValue("cap", "\"" + path + "\" --startup", RegistryValueKind.String);
        }
        else key.DeleteValue("cap", false);
    }
}
