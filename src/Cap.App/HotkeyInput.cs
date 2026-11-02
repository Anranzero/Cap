using System;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using Cap.Platform;

namespace Cap.App;

internal sealed class HotkeyInput : TextBox
{
    private string beforeCapture;
    private bool showingModifiers;
    public string Value { get; private set; }
    public Func<HotkeyGesture, string?>? ValidateGesture { get; set; }
    public event Action<string?>? ValidationMessageChanged;

    public HotkeyInput(string value)
    {
        Value = beforeCapture = value; Text = value;
        IsReadOnly = true; IsReadOnlyCaretVisible = false; AllowDrop = false;
        ContextMenu = null; Cursor = Cursors.Hand; TextAlignment = TextAlignment.Center;
        ToolTip = "点击后直接按组合键；Esc 撤回本次录入";
        SetResourceReference(StyleProperty, typeof(TextBox));
        InputMethod.SetIsInputMethodEnabled(this, false);
    }

    protected override void OnGotKeyboardFocus(KeyboardFocusChangedEventArgs e)
    {
        base.OnGotKeyboardFocus(e);
        beforeCapture = Value; showingModifiers = false; Text = "请按组合键…";
        ValidationMessageChanged?.Invoke(null);
    }

    protected override void OnLostKeyboardFocus(KeyboardFocusChangedEventArgs e)
    {
        showingModifiers = false; Text = Value;
        base.OnLostKeyboardFocus(e);
    }

    protected override void OnPreviewMouseLeftButtonDown(MouseButtonEventArgs e)
    {
        Focus(); e.Handled = true;
    }

    protected override void OnPreviewKeyDown(KeyEventArgs e)
    {
        Key key = e.Key switch
        {
            Key.System => e.SystemKey,
            Key.ImeProcessed => e.ImeProcessedKey,
            Key.DeadCharProcessed => e.DeadCharProcessedKey,
            _ => e.Key
        };
        var modifiers = Keyboard.Modifiers;
        if (key == Key.Tab && (modifiers == ModifierKeys.None || modifiers == ModifierKeys.Shift))
        { base.OnPreviewKeyDown(e); return; }

        e.Handled = true;
        if (e.IsRepeat) return;
        if (key == Key.Escape && modifiers == ModifierKeys.None)
        {
            Value = beforeCapture; Text = Value;
            ValidationMessageChanged?.Invoke(null);
            MoveFocus(new TraversalRequest(FocusNavigationDirection.Next)); return;
        }
        if (key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin)
        {
            showingModifiers = true; Text = ModifierText((uint)modifiers) + "…"; return;
        }
        Capture(new HotkeyGesture((uint)modifiers, (uint)KeyInterop.VirtualKeyFromKey(key)));
    }

    protected override void OnPreviewKeyUp(KeyEventArgs e)
    {
        e.Handled = true;
        if (showingModifiers)
            Text = Keyboard.Modifiers == ModifierKeys.None ? Value : ModifierText((uint)Keyboard.Modifiers) + "…";
    }

    // Registered combinations arrive as WM_HOTKEY instead of an ordinary key event.
    public bool Capture(HotkeyGesture gesture)
    {
        if (!IsKeyboardFocusWithin) return false;
        showingModifiers = false;
        ValidationMessageChanged?.Invoke(null);
        if (gesture.Modifiers == 0) { Text = "请同时按住修饰键"; return true; }
        string key = gesture.VirtualKey is >= 0x30 and <= 0x39
            ? ((char)gesture.VirtualKey).ToString()
            : KeyInterop.KeyFromVirtualKey((int)gesture.VirtualKey).ToString();
        string candidate = ModifierText(gesture.Modifiers) + key;
        try
        {
            if (HotkeyGesture.Parse(candidate) != gesture) throw new ArgumentException("无法映射此按键。");
            string? conflict = ValidateGesture?.Invoke(gesture);
            if (conflict != null)
            {
                Text = Value;
                ValidationMessageChanged?.Invoke(conflict);
                return true;
            }
            Value = candidate; Text = Value;
        }
        catch (ArgumentException) { Text = "此组合不可用，请重按"; }
        return true;
    }

    private static string ModifierText(uint modifiers) =>
        ((modifiers & 2) != 0 ? "Ctrl+" : "") + ((modifiers & 1) != 0 ? "Alt+" : "") +
        ((modifiers & 4) != 0 ? "Shift+" : "") + ((modifiers & 8) != 0 ? "Win+" : "");
}
