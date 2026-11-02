using System;
using System.Collections.Specialized;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Forms.Integration;
using System.Windows.Media;
using System.Windows.Threading;
using Cap.Core;
using Microsoft.Win32;

namespace Cap.App;

internal sealed class PreviewWindow : Window
{
    private readonly MediaElement? media;
    private readonly global::System.Windows.Forms.PictureBox? gif;
    private readonly WindowsFormsHost? gifHost;
    private readonly DispatcherTimer? progressTimer;
    private bool playing = true;
    public PreviewWindow(RecordingResult result)
    {
        Ui.PrepareWindow(this);
        Title = "cap · 录制预览"; Width = 880; Height = 640; MinWidth = 640; MinHeight = 420; WindowStartupLocation = WindowStartupLocation.CenterScreen;
        var root = new DockPanel();
        var titleBar = Ui.WindowHeader(this); DockPanel.SetDock(titleBar, Dock.Top); root.Children.Add(titleBar);
        var content = new Grid { Margin = new Thickness(20, 8, 20, 20) };
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.Children.Add(content);
        bool isGif = Path.GetExtension(result.FilePath).Equals(".gif", StringComparison.OrdinalIgnoreCase);
        var canvas = new Border { CornerRadius = new CornerRadius(16), Background = Ui.Brush("#131315"), Padding = new Thickness(6) };
        content.Children.Add(canvas);
        var player = new Grid { Margin = new Thickness(0, 10, 0, 12), Height = 38 };
        player.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        player.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        player.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetRow(player, 1); content.Children.Add(player);
        var time = Ui.Hint((isGif ? string.Empty : "00:00 / ") + FormatTime(result.Duration));
        time.FontFamily = new FontFamily("Cascadia Mono,Consolas"); time.VerticalAlignment = VerticalAlignment.Center; time.Margin = new Thickness(12, 0, 0, 0);
        Grid.SetColumn(time, 2); player.Children.Add(time);
        var actions = new Grid(); actions.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) }); actions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetRow(actions, 2); content.Children.Add(actions);
        var metadata = Ui.Hint($"{Path.GetFileName(result.FilePath)}  ·  {new FileInfo(result.FilePath).Length / 1048576d:F2} MB");
        metadata.TextWrapping = TextWrapping.NoWrap; metadata.TextTrimming = TextTrimming.CharacterEllipsis; metadata.ToolTip = result.FilePath; metadata.VerticalAlignment = VerticalAlignment.Center; metadata.Margin = new Thickness(0, 0, 14, 0);
        actions.Children.Add(metadata);
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right };
        Grid.SetColumn(buttons, 1); actions.Children.Add(buttons);
        var save = Ui.Button("保存", () => SaveAs(result.FilePath), true); save.Content = ActionContent("save", "保存"); save.Margin = new Thickness(0, 0, 8, 0); save.ToolTip = "录制已自动保存，点击另存为其他位置"; buttons.Children.Add(save);
        var copy = new Button { Content = ActionContent("copy", "复制到剪贴板"), Margin = new Thickness(0), ToolTip = "复制录制文件，可粘贴到文件夹或聊天应用" };
        copy.Click += (_, _) => { if (CopyFile(result.FilePath)) copy.Content = ActionContent("check", "已复制"); }; buttons.Children.Add(copy);
        var context = new ContextMenu();
        var openFolder = new MenuItem { Header = "打开所在文件夹", Icon = Ui.Icon("folder", 16) };
        openFolder.Click += (_, _) => OpenFolder(result.FilePath); context.Items.Add(openFolder); content.ContextMenu = context;
        if (isGif)
        {
            gif = new global::System.Windows.Forms.PictureBox { Dock = global::System.Windows.Forms.DockStyle.Fill, SizeMode = global::System.Windows.Forms.PictureBoxSizeMode.Zoom, BackColor = global::System.Drawing.Color.FromArgb(19, 19, 21), Image = global::System.Drawing.Image.FromFile(result.FilePath) };
            gifHost = new WindowsFormsHost { Child = gif };
            canvas.Child = gifHost;
            var gifHint = Ui.Hint("GIF 动画 · 自动循环"); gifHint.Margin = new Thickness(0); gifHint.VerticalAlignment = VerticalAlignment.Center;
            Grid.SetColumnSpan(gifHint, 2); player.Children.Add(gifHint);
        }
        else
        {
            media = new MediaElement { Source = new Uri(result.FilePath), LoadedBehavior = MediaState.Manual, UnloadedBehavior = MediaState.Close, Stretch = Stretch.Uniform };
            media.MediaFailed += (_, e) => { Ui.Error(this, e.ErrorException); };
            canvas.Child = media;
            var playback = Ui.IconButton("pause", "暂停播放", () => { });
            playback.Width = 34; playback.Height = 34; playback.Margin = new Thickness(0, 0, 10, 0); playback.IsEnabled = false;
            playback.Click += (_, _) =>
            {
                playing = !playing; if (playing) media.Play(); else media.Pause();
                playback.Content = Ui.Icon(playing ? "pause" : "play", 18); playback.ToolTip = playing ? "暂停播放" : "继续播放";
                System.Windows.Automation.AutomationProperties.SetName(playback, playing ? "暂停播放" : "继续播放");
            };
            player.Children.Add(playback);
            var timeline = new Slider { Minimum = 0, Maximum = result.Duration.TotalSeconds > 0 ? result.Duration.TotalSeconds : 1, IsMoveToPointEnabled = true, VerticalAlignment = VerticalAlignment.Center, IsEnabled = false, ToolTip = "拖动以定位播放位置" };
            Grid.SetColumn(timeline, 1); player.Children.Add(timeline);
            bool seeking = false, updating = false;
            void ShowPosition(double seconds) => time.Text = FormatTime(TimeSpan.FromSeconds(seconds)) + " / " + FormatTime(TimeSpan.FromSeconds(timeline.Maximum));
            timeline.AddHandler(Thumb.DragStartedEvent, new DragStartedEventHandler((_, _) => seeking = true));
            timeline.AddHandler(Thumb.DragCompletedEvent, new DragCompletedEventHandler((_, _) => { seeking = false; media.Position = TimeSpan.FromSeconds(timeline.Value); }));
            timeline.ValueChanged += (_, _) =>
            {
                if (updating) return;
                ShowPosition(timeline.Value);
                if (!seeking) media.Position = TimeSpan.FromSeconds(timeline.Value);
            };
            media.MediaOpened += (_, _) =>
            {
                if (media.NaturalDuration.HasTimeSpan && media.NaturalDuration.TimeSpan.TotalSeconds > 0) timeline.Maximum = media.NaturalDuration.TimeSpan.TotalSeconds;
                timeline.IsEnabled = true; playback.IsEnabled = true; ShowPosition(0);
            };
            media.MediaEnded += (_, _) => { media.Position = TimeSpan.Zero; if (playing) media.Play(); };
            progressTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(200) };
            progressTimer.Tick += (_, _) =>
            {
                if (seeking) return;
                updating = true; timeline.Value = Math.Clamp(media.Position.TotalSeconds, 0, timeline.Maximum); updating = false;
                ShowPosition(timeline.Value);
            };
            Loaded += (_, _) => progressTimer.Start();
            Loaded += (_, _) => media.Play();
        }
        Content = root;
        Closed += (_, _) => { progressTimer?.Stop(); media?.Close(); if (gif != null) { gif.Image?.Dispose(); gif.Dispose(); } gifHost?.Dispose(); };
    }
    private static string FormatTime(TimeSpan duration) => duration.ToString(duration.TotalHours >= 1 ? @"hh\:mm\:ss" : @"mm\:ss");
    private static StackPanel ActionContent(string icon, string text)
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal };
        content.Children.Add(Ui.Icon(icon, 16));
        content.Children.Add(new TextBlock { Text = text, Margin = new Thickness(7, 0, 0, 0), VerticalAlignment = VerticalAlignment.Center });
        return content;
    }
    private void OpenFolder(string source)
    {
        try { global::System.Diagnostics.Process.Start(new global::System.Diagnostics.ProcessStartInfo("explorer.exe", "/select,\"" + source + "\"") { UseShellExecute = true }); }
        catch (Exception ex) { Ui.Error(this, ex); }
    }
    private bool CopyFile(string source)
    {
        try
        {
            Clipboard.SetFileDropList(new StringCollection { Path.GetFullPath(source) });
            return true;
        }
        catch (Exception ex) { Ui.Error(this, ex); return false; }
    }
    private void SaveAs(string source)
    {
        try
        {
            string extension = Path.GetExtension(source);
            var dialog = new SaveFileDialog { FileName = Path.GetFileName(source), InitialDirectory = Path.GetDirectoryName(source), Filter = extension == ".gif" ? "GIF 动画|*.gif" : "MP4 视频|*.mp4", DefaultExt = extension };
            if (dialog.ShowDialog(this) == true && !string.Equals(Path.GetFullPath(dialog.FileName), Path.GetFullPath(source), StringComparison.OrdinalIgnoreCase)) File.Copy(source, dialog.FileName, true);
        }
        catch (Exception ex) { Ui.Error(this, ex); }
    }
}
