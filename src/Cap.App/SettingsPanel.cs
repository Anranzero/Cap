using System.Windows;
using System.Windows.Controls;

namespace Cap.App;

internal static class SettingsPanel
{
    public static StackPanel Page(string title, string subtitle)
    {
        var panel = new StackPanel();
        var heading = Ui.Heading(title); heading.FontSize = 19; heading.Margin = new Thickness(0, 0, 0, 4);
        var hint = Ui.Hint(subtitle); hint.Margin = new Thickness(0, 0, 0, 14);
        panel.Children.Add(heading); panel.Children.Add(hint);
        return panel;
    }

    public static Border Card(params UIElement[] children)
    {
        var panel = new StackPanel();
        for (int index = 0; index < children.Length; index++)
        {
            if (index > 0)
            {
                var divider = new Border { Height = 1, Margin = new Thickness(0, 2, 0, 2) };
                divider.SetResourceReference(Border.BackgroundProperty, "BorderBrush"); panel.Children.Add(divider);
            }
            panel.Children.Add(children[index]);
        }
        var card = Ui.Card(panel, new Thickness(14, 4, 14, 4)); card.Margin = new Thickness(0, 0, 0, 9);
        return card;
    }

    public static Grid Row(string title, string? description, FrameworkElement control, double controlWidth = 180)
    {
        var row = new Grid { MinHeight = description == null ? 43 : 58 };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var label = new StackPanel { VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 6, 14, 6) };
        label.Children.Add(new TextBlock { Text = title, FontWeight = FontWeights.Medium });
        if (description != null)
        {
            var hint = Ui.Hint(description); hint.Margin = new Thickness(0, 4, 0, 0); label.Children.Add(hint);
        }
        row.Children.Add(label);
        control.Width = controlWidth; control.Margin = new Thickness(0, 5, 0, 5); control.VerticalAlignment = VerticalAlignment.Center;
        Grid.SetColumn(control, 1); row.Children.Add(control);
        return row;
    }

    public static Grid Switch(string title, string? description, CheckBox check)
    {
        check.Content = null;
        System.Windows.Automation.AutomationProperties.SetName(check, title);
        return Row(title, description, check, 52);
    }

    public static StackPanel Field(string title, FrameworkElement control, string? description = null)
    {
        var field = new StackPanel();
        field.Children.Add(new TextBlock { Text = title, Margin = new Thickness(0, 0, 0, 8), FontWeight = FontWeights.Medium });
        control.Margin = new Thickness(0); control.MinHeight = 32; field.Children.Add(control);
        if (description != null)
        {
            var hint = Ui.Hint(description); hint.Margin = new Thickness(0, 6, 0, 0); field.Children.Add(hint);
        }
        return field;
    }

    public static Grid Pair(UIElement left, UIElement right, double gap = 18)
    {
        var grid = new Grid();
        grid.ColumnDefinitions.Add(new ColumnDefinition());
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(gap) });
        grid.ColumnDefinitions.Add(new ColumnDefinition());
        grid.Children.Add(left); Grid.SetColumn(right, 2); grid.Children.Add(right); return grid;
    }

}
