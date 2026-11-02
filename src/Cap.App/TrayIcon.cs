using System.Drawing;
using System.Windows;

namespace Cap.App;

internal static class TrayIcon
{
    public static Icon Create()
    {
        using var stream = Application.GetResourceStream(new System.Uri("pack://application:,,,/cap;component/Assets/cap.ico"))!.Stream;
        using var icon = new Icon(stream, 32, 32); return (Icon)icon.Clone();
    }
}
