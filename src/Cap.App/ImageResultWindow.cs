using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Microsoft.Win32;

namespace Cap.App;

internal sealed class ImageResultWindow : Window
{
    public ImageResultWindow(BitmapSource image, string directory)
    {
        Ui.PrepareWindow(this); Title = "cap · 长截图"; Width = 800; Height = 680; MinWidth = 480; MinHeight = 360; WindowStartupLocation = WindowStartupLocation.CenterScreen;
        var root = new DockPanel(); var header = Ui.WindowHeader(this); DockPanel.SetDock(header, Dock.Top); root.Children.Add(header);
        var actions = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(18) };
        actions.Children.Add(Ui.Button("保存", () => Save(image, directory), true));
        var copy = Ui.Button("复制", () => { try { Clipboard.SetImage(image); } catch (Exception ex) { Ui.Error(this, ex); } }); copy.Margin = new Thickness(8, 0, 0, 0); actions.Children.Add(copy);
        DockPanel.SetDock(actions, Dock.Bottom); root.Children.Add(actions);
        var view = new Image { Source = image, Width = Math.Min(740, image.PixelWidth), Stretch = Stretch.Uniform, VerticalAlignment = VerticalAlignment.Top };
        root.Children.Add(new ScrollViewer { Content = view, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled, Margin = new Thickness(18, 0, 18, 0) });
        Content = root;
    }
    private void Save(BitmapSource image, string directory)
    {
        string? temporary = null;
        try
        {
            Directory.CreateDirectory(directory);
            var picker = new SaveFileDialog { InitialDirectory = directory, FileName = "cap-scroll-" + DateTime.Now.ToString("yyyyMMdd-HHmmss") + ".png", Filter = "PNG 图片|*.png|JPEG 图片|*.jpg", DefaultExt = ".png" };
            if (picker.ShowDialog(this) != true) return;
            BitmapEncoder encoder = Path.GetExtension(picker.FileName).Equals(".png", StringComparison.OrdinalIgnoreCase) ? new PngBitmapEncoder() : new JpegBitmapEncoder { QualityLevel = 94 };
            encoder.Frames.Add(BitmapFrame.Create(image)); temporary = picker.FileName + "." + Guid.NewGuid().ToString("N") + ".tmp";
            using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None)) { encoder.Save(stream); stream.Flush(true); }
            File.Move(temporary, picker.FileName, true); temporary = null;
        }
        catch (Exception ex) { Ui.Error(this, ex); }
        finally { if (temporary != null && File.Exists(temporary)) { try { File.Delete(temporary); } catch (IOException ex) { Cap.Platform.AppLog.Error(ex); } } }
    }
}
