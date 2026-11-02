using System;
using System.Globalization;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Input;

namespace Cap.App;

internal sealed class TextExtractWindow : Window
{
    public TextExtractWindow(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        Title = "cap · 提取文字"; Width = 680; Height = 500; MinWidth = 420; MinHeight = 300;
        WindowStartupLocation = WindowStartupLocation.CenterScreen;
        Ui.PrepareWindow(this);
        var root = new DockPanel();
        var header = Ui.WindowHeader(this); DockPanel.SetDock(header, Dock.Top); root.Children.Add(header);
        var body = new Grid { Margin = new Thickness(22, 8, 22, 22) };
        body.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        body.RowDefinitions.Add(new RowDefinition());
        body.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var intro = new StackPanel();
        intro.Children.Add(Ui.Heading("提取文字"));
        intro.Children.Add(Ui.Hint(text.Length == 0 ? "未识别到文字，可重新框选清晰的文字区域。" : "选择所需文字，或复制全部内容。"));
        body.Children.Add(intro);
        var content = new TextBox
        {
            Text = text, IsReadOnly = true, AcceptsReturn = true, AcceptsTab = true,
            TextWrapping = TextWrapping.Wrap, VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled, FontSize = 16,
            BorderThickness = new Thickness(0), Padding = new Thickness(4),
            VerticalContentAlignment = VerticalAlignment.Top, IsReadOnlyCaretVisible = true
        };
        content.SetResourceReference(Control.BackgroundProperty, "SurfaceBrush");
        content.SetResourceReference(Control.ForegroundProperty, "TextBrush");
        AutomationProperties.SetName(content, "识别结果，可选择并复制文字");
        var card = Ui.Card(content); Grid.SetRow(card, 1); body.Children.Add(card);
        var footer = new Grid { Margin = new Thickness(2, 16, 0, 0) };
        footer.ColumnDefinitions.Add(new ColumnDefinition());
        footer.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var count = Ui.Hint($"{new StringInfo(text).LengthInTextElements:N0} 字符 · 本地识别");
        count.VerticalAlignment = VerticalAlignment.Center; count.Margin = new Thickness(0);
        footer.Children.Add(count);
        var actions = new StackPanel { Orientation = Orientation.Horizontal };
        var close = Ui.Button("关闭", Close); close.Margin = new Thickness(0, 0, 8, 0); close.MinWidth = 72;
        actions.Children.Add(close);
        Button copy = Ui.Button("复制全部", () =>
        {
            try { Clipboard.SetText(text); }
            catch (Exception ex) when (ex is System.Runtime.InteropServices.ExternalException or System.Threading.ThreadStateException)
            {
                Ui.Error(this, new InvalidOperationException("无法写入剪贴板，请稍后再试。", ex));
            }
        }, true);
        copy.IsEnabled = text.Length > 0; copy.MinWidth = 100; actions.Children.Add(copy);
        Grid.SetColumn(actions, 1); footer.Children.Add(actions);
        Grid.SetRow(footer, 2); body.Children.Add(footer);
        root.Children.Add(body); Content = root;
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) { Close(); e.Handled = true; } };
    }
}
