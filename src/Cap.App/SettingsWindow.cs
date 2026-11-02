using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using Cap.Platform;
using Microsoft.Win32;

namespace Cap.App;

internal sealed class SettingsWindow : Window
{
    private readonly AppSettings original;
    private readonly Func<AppSettings, bool> save;
    private string appliedTheme;
    private readonly TextBox folder, gifFps, gifWidth, fontSize;
    private readonly ColorPickerButton color;
    private readonly HotkeyInput screenshot, video, gif;
    private readonly ComboBox theme, fps, coordinateMode;
    private readonly CheckBox startup, closeToTray, cursor, clicks, coordinates;
    private readonly ContentControl pageHost = new();
    private readonly Button[] navigation = new Button[4];
    private readonly FrameworkElement[] pages;
    public event Action? ExitRequested;
    public event Action? RecoveryRequested;
    public event Action? AdvancedRecordingRequested;
    public event Action<string>? ThemeChangeRequested;

    public SettingsWindow(AppSettings settings, Func<AppSettings, bool> save)
    {
        Ui.PrepareWindow(this);
        original = settings; this.save = save; appliedTheme = settings.Theme;
        Title = $"cap · 设置 · v{typeof(App).Assembly.GetName().Version!.ToString(3)}"; Width = 620; Height = 560; MinWidth = 570; MinHeight = 470; WindowStartupLocation = WindowStartupLocation.CenterScreen;
        folder = Ui.Input(settings.SaveDirectory);
        screenshot = new(settings.ScreenshotHotkey); video = new(settings.RecordingHotkey); gif = new(settings.GifHotkey);
        theme = Ui.Choice("深色", "浅色"); theme.SelectedIndex = settings.Theme == "Dark" ? 0 : 1;
        startup = Ui.Check("开机自动运行（当前用户）", settings.RunAtStartup); closeToTray = Ui.Check("关闭窗口时最小化到托盘", settings.CloseToTray);
        fps = Ui.Choice("15", "24", "30", "60"); fps.SelectedItem = settings.FramesPerSecond.ToString();
        gifFps = Ui.Input(settings.GifFramesPerSecond.ToString()); gifWidth = Ui.Input(settings.GifMaxWidth.ToString());
        cursor = Ui.Check("录制鼠标指针", settings.ShowCursor); clicks = Ui.Check("鼠标点击高亮", settings.HighlightClicks);
        coordinates = Ui.Check("将鼠标坐标写入视频 / GIF", settings.ShowCoordinates);
        coordinateMode = Ui.Choice("屏幕绝对坐标", "录制区域相对坐标"); coordinateMode.SelectedIndex = settings.RelativeCoordinates ? 1 : 0;
        fontSize = Ui.Input(settings.CoordinateFontSize.ToString()); color = new ColorPickerButton(settings.CoordinateColor);
        pages = new FrameworkElement[] { GeneralPage(), HotkeysPage(), RecordingPage(), MousePage() };

        var layout = new Grid(); layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); layout.RowDefinitions.Add(new RowDefinition()); layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        layout.Children.Add(Ui.WindowHeader(this));
        var tabs = Categories(); Grid.SetRow(tabs, 1); layout.Children.Add(tabs);
        var scroll = new ScrollViewer { Content = pageHost, Padding = new Thickness(22, 16, 22, 12), VerticalScrollBarVisibility = ScrollBarVisibility.Auto, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled };
        Grid.SetRow(scroll, 2); layout.Children.Add(scroll);
        var footer = new Border { Padding = new Thickness(22, 12, 22, 12), BorderThickness = new Thickness(0, 1, 0, 0) };
        footer.SetResourceReference(Border.BackgroundProperty, "SurfaceBrush"); footer.SetResourceReference(Border.BorderBrushProperty, "BorderBrush");
        var actions = new DockPanel(); var note = Ui.Button("退出程序", () => ExitRequested?.Invoke()); note.Margin = new Thickness(0); note.Padding = new Thickness(0); note.Background = System.Windows.Media.Brushes.Transparent; note.BorderThickness = new Thickness(0); note.SetResourceReference(Control.ForegroundProperty, "MutedBrush"); note.VerticalAlignment = VerticalAlignment.Center;
        note.HorizontalAlignment = HorizontalAlignment.Left;
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right };
        var cancel = Ui.Button("取消", Close); cancel.Margin = new Thickness(0, 0, 10, 0); cancel.MinWidth = 76;
        var apply = Ui.Button("保存设置", Save, true); apply.Margin = new Thickness(0); apply.MinWidth = 110;
        buttons.Children.Add(cancel); buttons.Children.Add(apply); DockPanel.SetDock(buttons, Dock.Right); actions.Children.Add(buttons); actions.Children.Add(note);
        footer.Child = actions; Grid.SetRow(footer, 3); layout.Children.Add(footer);
        Content = layout; SelectPage(0);
        theme.SelectionChanged += ChangeTheme;
    }

    private void ChangeTheme(object sender, SelectionChangedEventArgs e)
    {
        if (theme.SelectedIndex is not (0 or 1)) return;
        string selectedTheme = theme.SelectedIndex == 0 ? "Dark" : "Light";
        if (selectedTheme == appliedTheme) return;
        try
        {
            Ui.Theme(selectedTheme == "Dark");
            ThemeChangeRequested?.Invoke(selectedTheme);
            appliedTheme = selectedTheme;
        }
        catch (Exception ex)
        {
            theme.SelectedIndex = appliedTheme == "Dark" ? 0 : 1;
            Ui.Theme(appliedTheme == "Dark");
            Ui.Error(this, ex);
        }
    }

    private FrameworkElement Categories()
    {
        var menu = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(22, 5, 22, 9) };
        string[] titles = { "常规", "快捷键", "录制", "鼠标与坐标" };
        for (int index = 0; index < titles.Length; index++)
        {
            int selected = index; var button = Ui.Button(titles[index], () => SelectPage(selected));
            button.Padding = new Thickness(15, 8, 15, 8);
            button.Margin = new Thickness(0, 0, 5, 0); button.BorderThickness = new Thickness(0); button.MinHeight = 34;
            System.Windows.Automation.AutomationProperties.SetName(button, titles[index]);
            navigation[index] = button; menu.Children.Add(button);
        }
        var tabs = new Border { Child = menu, BorderThickness = new Thickness(0, 0, 0, 1) };
        tabs.SetResourceReference(Border.BackgroundProperty, "WindowBrush"); tabs.SetResourceReference(Border.BorderBrushProperty, "BorderBrush"); return tabs;
    }

    private void SelectPage(int selected)
    {
        pageHost.Content = pages[selected];
        for (int index = 0; index < navigation.Length; index++)
        {
            navigation[index].SetResourceReference(Control.BackgroundProperty, index == selected ? "AccentSoftBrush" : "WindowBrush");
            navigation[index].SetResourceReference(Control.ForegroundProperty, index == selected ? "AccentBrush" : "MutedBrush");
            navigation[index].FontWeight = index == selected ? FontWeights.SemiBold : FontWeights.Normal;
        }
    }

    private FrameworkElement GeneralPage()
    {
        var page = SettingsPanel.Page("常规", "让 cap 更贴合你的使用习惯。");
        var folderRow = new DockPanel();
        var browse = Ui.IconButton("folder", "选择文件夹", BrowseFolder); browse.Margin = new Thickness(10, 0, 0, 0); browse.MinWidth = 40;
        DockPanel.SetDock(browse, Dock.Right); folderRow.Children.Add(browse); folder.Margin = new Thickness(0); folderRow.Children.Add(folder);
        var location = SettingsPanel.Field("默认保存目录", folderRow); location.Margin = new Thickness(0, 8, 0, 8);
        page.Children.Add(SettingsPanel.Card(location));
        page.Children.Add(SettingsPanel.Card(SettingsPanel.Row("外观主题", "切换即生效，自动保存", theme)));
        page.Children.Add(SettingsPanel.Card(SettingsPanel.Switch("开机自动运行", null, startup), SettingsPanel.Switch("关闭后留在托盘", null, closeToTray)));
        var note = Ui.Hint("移动 cap.exe 后，请重新开启开机自动运行。"); note.Margin = new Thickness(2, 2, 0, 0); page.Children.Add(note);
        return page;
    }

    private FrameworkElement HotkeysPage()
    {
        var page = SettingsPanel.Page("快捷键", "不用切换窗口，随时捕捉灵感。");
        page.Children.Add(SettingsPanel.Card(SettingsPanel.Row("截图", "框选、标注并保存", HotkeyField(screenshot, "截图")), SettingsPanel.Row("视频录制", "开始或停止 MP4 录制", HotkeyField(video, "视频录制")), SettingsPanel.Row("GIF 录制", "开始或停止 GIF 录制", HotkeyField(gif, "GIF 录制"))));
        var hint = Ui.Hint("点击右侧后直接按组合键，Esc 撤回本次录入。三个快捷键不能相同。\n录制中再次按对应快捷键，即可停止并保存。"); hint.Margin = new Thickness(2, 8, 0, 0); page.Children.Add(hint);
        return page;
    }

    private FrameworkElement HotkeyField(HotkeyInput input, string name)
    {
        var field = new StackPanel();
        input.Margin = new Thickness(0);
        System.Windows.Automation.AutomationProperties.SetName(input, name + "快捷键");
        var message = Ui.Hint(""); message.Margin = new Thickness(0, 5, 0, 0);
        message.Foreground = Ui.Brush("#E95368"); message.Visibility = Visibility.Collapsed;
        input.ValidateGesture = gesture =>
        {
            foreach (var (other, label) in new[] { (screenshot, "截图"), (video, "视频录制"), (gif, "GIF 录制") })
                if (!ReferenceEquals(input, other) && HotkeyGesture.Parse(other.Value) == gesture)
                    return $"与“{label}”重复，已保留原快捷键。";
            return null;
        };
        input.ValidationMessageChanged += text =>
        {
            message.Text = text ?? "";
            message.Visibility = text == null ? Visibility.Collapsed : Visibility.Visible;
            System.Windows.Automation.AutomationProperties.SetHelpText(input, text ?? "点击后直接按组合键");
        };
        field.Children.Add(input); field.Children.Add(message);
        return field;
    }

    public bool CaptureHotkey(HotkeyGesture gesture) => IsActive && IsVisible &&
        (screenshot.Capture(gesture) || video.Capture(gesture) || gif.Capture(gesture));

    private FrameworkElement RecordingPage()
    {
        var page = SettingsPanel.Page("录制", "直接录制使用这里保存的设置。");
        page.Children.Add(SettingsPanel.Card(SettingsPanel.Row("视频帧率", "MP4 · 每秒帧数", fps)));
        page.Children.Add(SettingsPanel.Card(SettingsPanel.Row("GIF 帧率", "每秒 1–60 帧", gifFps), SettingsPanel.Row("GIF 最大宽度", "64–7680 px · 保持原有比例", gifWidth)));
        var hint = Ui.Hint("较高帧率让动作更流畅，也会增加文件大小。"); hint.Margin = new Thickness(2, 8, 0, 0); page.Children.Add(hint);
        var advanced = Ui.Button("窗口 / 全屏录制", () => AdvancedRecordingRequested?.Invoke()); advanced.Margin = new Thickness(0, 14, 0, 0); advanced.HorizontalAlignment = HorizontalAlignment.Left; page.Children.Add(advanced);
        var recover = Ui.Button("恢复中断的录制", () => RecoveryRequested?.Invoke()); recover.Margin = new Thickness(0, 14, 0, 0); recover.HorizontalAlignment = HorizontalAlignment.Left; page.Children.Add(recover);
        return page;
    }

    private FrameworkElement MousePage()
    {
        var page = SettingsPanel.Page("鼠标与坐标", "突出操作过程，让演示更容易跟随。");
        page.Children.Add(SettingsPanel.Card(SettingsPanel.Switch("录制鼠标指针", null, cursor), SettingsPanel.Switch("鼠标点击高亮", null, clicks), SettingsPanel.Switch("显示鼠标坐标", null, coordinates)));
        var appearance = SettingsPanel.Pair(SettingsPanel.Field("字号 · 10–72", fontSize), SettingsPanel.Field("颜色", color)); appearance.Margin = new Thickness(0, 8, 0, 10);
        var options = SettingsPanel.Card(SettingsPanel.Row("坐标方式", null, coordinateMode), appearance);
        options.IsEnabled = coordinates.IsChecked == true;
        coordinates.Checked += (_, _) => options.IsEnabled = true; coordinates.Unchecked += (_, _) => options.IsEnabled = false;
        page.Children.Add(options); return page;
    }

    private void BrowseFolder()
    {
        var dialog = new OpenFolderDialog { Title = "默认保存目录", InitialDirectory = Directory.Exists(folder.Text) ? folder.Text : Environment.GetFolderPath(Environment.SpecialFolder.MyPictures) };
        if (dialog.ShowDialog(this) == true) folder.Text = dialog.FolderName;
    }
    private void Save()
    {
        try
        {
            var next = original with
            {
                SaveDirectory = folder.Text.Trim(), ScreenshotHotkey = screenshot.Value, RecordingHotkey = video.Value, GifHotkey = gif.Value,
                Theme = theme.SelectedIndex == 0 ? "Dark" : "Light", RunAtStartup = startup.IsChecked == true, CloseToTray = closeToTray.IsChecked == true,
                FramesPerSecond = int.Parse((string)fps.SelectedItem), GifFramesPerSecond = int.Parse(gifFps.Text), GifMaxWidth = int.Parse(gifWidth.Text),
                ShowCursor = cursor.IsChecked == true, HighlightClicks = clicks.IsChecked == true, ShowCoordinates = coordinates.IsChecked == true,
                RelativeCoordinates = coordinateMode.SelectedIndex == 1, CoordinateFontSize = int.Parse(fontSize.Text), CoordinateColor = color.Value
            };
            next.Validate();
            if (save(next)) Close();
        }
        catch (Exception ex) { Ui.Error(this, ex); }
    }
}
