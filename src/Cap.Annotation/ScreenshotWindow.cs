using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Cap.Capture;
using Cap.Core;
using Microsoft.Win32;

namespace Cap.Annotation;

internal sealed class ScreenshotWindow : Window
{
    private readonly IReadOnlyList<DesktopWindow> windows;
    private readonly string saveDirectory;
    private readonly bool dark;
    private readonly bool regionOnly;
    private readonly Canvas canvas;
    private readonly EditorSurface surface;
    private readonly EditorToolbar toolbar;
    private readonly CaptureModeBar modeBar;
    private readonly TextBlock hint;
    private Point? modeBarPosition;
    private Vector modeBarDragOffset;
    private PixelRect modeBarMonitor;
    private Point? toolbarPosition;
    private Vector toolbarDragOffset;
    private PixelRect toolbarMonitor;
    private bool dragging;
    private bool finishing;
    private int dragHandle;
    private Point start;
    private Point current;
    private PixelRect original;
    private PixelRect snap;
    private List<Point> penPoints = [];
    private HwndSource? hwndSource;
    private ScrollingCaptureWindow? scrollSession;
    private bool scrolling;
    private bool scrollArmed;
    private string? hintNote;
    private readonly ColorPickerPanel colorPicker;
    private readonly ScrollingStartBar scrollStartBar;
    private readonly TextExtractPanel textPanel;
    private readonly RecordingStartPanel recordingPanel;
    private CancellationTokenSource? ocrCancellation;
    private Border? scrollHelp;
    private const string DefaultHint = "拖动框选或单击窗口 · 标记、复制或选择功能 · Esc 取消";
    private readonly bool preselectAll;

    public CaptureMode Mode => modeBar.SelectedMode;
    public PixelRect Desktop { get; }
    public AnnotationDocument Document { get; }
    public PixelRect Selection { get; private set; }
    public PixelRect Hover { get; private set; }
    public PaintCommand? Preview { get; private set; }
    /// <summary>True while the in-place scrolling capture owns the selection.</summary>
    public bool Scrolling => scrolling;
    public AnnotationTool Tool { get; private set; } = AnnotationTool.Select;
    public Color InkColor { get; set; } = Color.FromRgb(0xF0, 0x5B, 0x56);
    public double InkThickness { get; set; } = 3;
    /// <summary>Highlighter (///) opacity, matching Feishu's default 30%.</summary>
    public double MarkerOpacity { get; set; } = .3;
    /// <summary>Text style: 0 plain, 1 outlined, 2 serif (Feishu's three T buttons).</summary>
    public int TextStyle { get; set; }
    public ScreenshotResult? Result { get; private set; }
    /// <summary>Set when the crop tool asks the workflow to reopen a smaller overlay.</summary>
    public PixelRect? CropRegion { get; private set; }
    /// <summary>Baked pixels of the cropped region handed to the reopened overlay.</summary>
    public PixelFrame? CroppedFrame { get; private set; }
    public PixelRect? RegionResult { get; private set; }

    public ScreenshotWindow(PixelRect desktop, PixelFrame frame, IReadOnlyList<DesktopWindow> windows,
        string saveDirectory, bool dark, bool regionOnly)
        : this(desktop, frame, windows, saveDirectory, dark, regionOnly, regionOnly ? CaptureMode.Recording : CaptureMode.Screenshot) { }

    public ScreenshotWindow(PixelRect desktop, PixelFrame frame, IReadOnlyList<DesktopWindow> windows,
        string saveDirectory, bool dark, bool regionOnly, CaptureMode initialMode, bool preselectAll = false)
    {
        this.preselectAll = preselectAll;
        Desktop = desktop;
        Document = new AnnotationDocument(frame);
        this.windows = windows;
        this.saveDirectory = saveDirectory;
        this.dark = dark;
        this.regionOnly = regionOnly;
        Title = regionOnly ? "Cap 选择录制区域" : "Cap 截图";
        WindowStyle = WindowStyle.None;
        ResizeMode = ResizeMode.NoResize;
        WindowStartupLocation = WindowStartupLocation.Manual;
        ShowInTaskbar = false;
        Topmost = true;
        Background = Brushes.Black;
        Left = desktop.X;
        Top = desktop.Y;
        Width = Math.Max(1, desktop.Width);
        Height = Math.Max(1, desktop.Height);
        canvas = new Canvas { ClipToBounds = true };
        Content = canvas;
        surface = new EditorSurface(this) { Focusable = true, Cursor = Cursors.Cross };
        canvas.Children.Add(surface);
        toolbar = new EditorToolbar(this, dark, regionOnly) { Visibility = Visibility.Collapsed };
        canvas.Children.Add(toolbar);
        toolbar.DragStarted += () => toolbarDragOffset = CursorPoint() - new Point(
            Canvas.GetLeft(toolbar) * Desktop.Width / ActualWidth, Canvas.GetTop(toolbar) * Desktop.Height / ActualHeight);
        toolbar.DragMoved += () =>
        {
            toolbarPosition = CursorPoint() - toolbarDragOffset;
            toolbarMonitor = NativeDesktop.MonitorAt(NativeDesktop.Cursor()).Intersect(Desktop);
            if (toolbarMonitor.IsEmpty) toolbarMonitor = Desktop;
            Update();
        };
        modeBar = new CaptureModeBar(initialMode) { Visibility = Visibility.Collapsed };
        modeBar.Chosen += ChooseMode; canvas.Children.Add(modeBar);
        modeBar.DragStarted += () => modeBarDragOffset = CursorPoint() - new Point(
            Canvas.GetLeft(modeBar) * Desktop.Width / ActualWidth, Canvas.GetTop(modeBar) * Desktop.Height / ActualHeight);
        modeBar.DragMoved += () =>
        {
            modeBarPosition = CursorPoint() - modeBarDragOffset;
            modeBarMonitor = NativeDesktop.MonitorAt(NativeDesktop.Cursor()).Intersect(Desktop);
            if (modeBarMonitor.IsEmpty) modeBarMonitor = Desktop;
            Update();
        };
        hint = new TextBlock
        {
            Text = DefaultHint,
            Foreground = Brushes.White, Background = new SolidColorBrush(Color.FromArgb(220, 25, 28, 34)),
            FontSize = 13, Padding = new Thickness(12, 8, 12, 8), IsHitTestVisible = false
        };
        canvas.Children.Add(hint);
        colorPicker = new ColorPickerPanel(this);
        canvas.Children.Add(colorPicker);
        scrollStartBar = new ScrollingStartBar { Visibility = Visibility.Collapsed };
        scrollStartBar.StartRequested += EnterScrollingMode;
        scrollStartBar.HelpRequested += ToggleScrollHelp;
        canvas.Children.Add(scrollStartBar);
        textPanel = new TextExtractPanel(dark);
        textPanel.CloseRequested += CloseTextPanel;
        canvas.Children.Add(textPanel);
        recordingPanel = new RecordingStartPanel();
        recordingPanel.StartRequested += StartRecordingFromPanel;
        canvas.Children.Add(recordingPanel);
        Closed += (_, _) => ocrCancellation?.Cancel();
        surface.MouseLeftButtonDown += PointerDown;
        surface.MouseMove += PointerMove;
        surface.MouseLeftButtonUp += PointerUp;
        surface.MouseRightButtonDown += (_, e) =>
        {
            e.Handled = true;
            if (scrolling) return;
            dragging = false;
            surface.ReleaseMouseCapture();
            Preview = null;
            if (Selection.IsEmpty) Cancel();
            else
            {
                Selection = default;
                modeBarPosition = null;
                toolbarPosition = null;
                UpdateHover(CursorPoint());
                SetTool(AnnotationTool.Select);
            }
        };
        surface.LostMouseCapture += (_, _) => { if (dragging) { dragging = false; Preview = null; Update(); } };
        PreviewKeyDown += OnKey;
        SizeChanged += (_, _) => { surface.Width = ActualWidth; surface.Height = ActualHeight; Update(); };
        SourceInitialized += (_, _) =>
        {
            nint handle = new WindowInteropHelper(this).Handle;
            CaptureService.ExcludeWindow(handle);
            hwndSource = HwndSource.FromHwnd(handle);
            hwndSource?.AddHook(WindowMessage);
        };
        Loaded += (_, _) =>
        {
            NativeDesktop.Place(this, Desktop);
            surface.Focus();
            UpdateHover(CursorPoint());
            if (preselectAll && Selection.IsEmpty)
                Selection = new PixelRect(0, 0, Desktop.Width, Desktop.Height);
            Update();
        };
        Closed += (_, _) => { scrollSession?.Close(); hwndSource?.RemoveHook(WindowMessage); };
    }

    private nint WindowMessage(nint hwnd, int message, nint wParam, nint lParam, ref bool handled)
    {
        if (message == 0x02E0) // WM_DPICHANGED: preserve the physical virtual-desktop rectangle.
            Dispatcher.BeginInvoke(DispatcherPriority.Loaded, new Action(() => { if (IsVisible) NativeDesktop.Place(this, Desktop); ApplyCutout(); }));
        return 0;
    }

    private Point CursorPoint()
    {
        Point position = NativeDesktop.Cursor();
        return new Point(Math.Clamp(position.X - Desktop.X, 0, Desktop.Width), Math.Clamp(position.Y - Desktop.Y, 0, Desktop.Height));
    }

    private Point WithinSelection(Point point) => new(Math.Clamp(point.X, Selection.X, Selection.Right - 1), Math.Clamp(point.Y, Selection.Y, Selection.Bottom - 1));

    private void UpdateHover(Point point)
    {
        Hover = default;
        int x = (int)point.X + Desktop.X, y = (int)point.Y + Desktop.Y;
        DesktopWindow? window = windows.FirstOrDefault(w => w.Bounds.Contains(x, y));
        if (window is not null)
        {
            PixelRect bounds = window.Bounds.Intersect(Desktop);
            if (!bounds.IsEmpty) Hover = bounds with { X = bounds.X - Desktop.X, Y = bounds.Y - Desktop.Y };
        }
    }

    private void PointerDown(object sender, MouseButtonEventArgs e)
    {
        if (finishing || scrolling) return;
        surface.Focus();
        start = current = CursorPoint();
        hintNote = null;
        original = Selection;
        UpdateHover(start);
        snap = Hover;
        if (Tool == AnnotationTool.Select || Selection.IsEmpty)
        {
            dragHandle = SelectionGeometry.Hit(Selection, start, 7 * Desktop.Width / Math.Max(1, ActualWidth));
            if (dragHandle == -2) { dragHandle = -99; Selection = default; modeBarPosition = null; toolbarPosition = null; }
        }
        else
        {
            if (!Selection.Contains((int)start.X, (int)start.Y)) return;
            start = current = WithinSelection(start);
            if (Tool == AnnotationTool.Text)
            {
                var prompt = new TextPrompt(this, dark);
                if (prompt.ShowDialog() == true) Document.Add(new PaintCommand(Tool, start, start, InkColor, InkThickness, Text: prompt.Value));
                Update();
                e.Handled = true;
                return;
            }
            if (Tool == AnnotationTool.Number)
            {
                Document.Add(new PaintCommand(Tool, start, start, InkColor, InkThickness, Number: Document.NextNumber));
                Update();
                e.Handled = true;
                return;
            }
            dragHandle = -100;
            penPoints = [start];
        }
        dragging = true;
        surface.CaptureMouse();
        Update();
        e.Handled = true;
    }

    private void PointerMove(object sender, MouseEventArgs e)
    {
        if (scrolling) return;
        current = CursorPoint();
        UpdateColorPicker(current);
        if (!dragging)
        {
            if (Selection.IsEmpty) UpdateHover(current);
            else if (Tool == AnnotationTool.Select)
            {
                int handle = SelectionGeometry.Hit(Selection, current, 7 * Desktop.Width / Math.Max(1, ActualWidth));
                surface.Cursor = handle switch { 0 or 4 => Cursors.SizeNWSE, 2 or 6 => Cursors.SizeNESW, 1 or 5 => Cursors.SizeNS, 3 or 7 => Cursors.SizeWE, -1 => Cursors.SizeAll, _ => Cursors.Cross };
            }
            surface.InvalidateVisual();
            return;
        }
        if (dragHandle == -99) Selection = SelectionGeometry.Drag(start, current, Desktop.Width, Desktop.Height);
        else if (dragHandle == -100)
        {
            current = WithinSelection(current);
            if (Tool is AnnotationTool.Pen or AnnotationTool.Marker && (penPoints[^1] - current).Length >= .75) penPoints.Add(current);
            Preview = new PaintCommand(Tool, start, current, InkColor, InkThickness,
                Tool is AnnotationTool.Pen or AnnotationTool.Marker ? penPoints.ToArray() : null,
                Opacity: Tool == AnnotationTool.Marker ? MarkerOpacity : 1, TextStyle: Tool == AnnotationTool.Text ? TextStyle : 0);
        }
        else Selection = SelectionGeometry.Adjust(original, start, current, dragHandle, Desktop.Width, Desktop.Height);
        Update();
        e.Handled = true;
    }

    private void PointerUp(object sender, MouseButtonEventArgs e)
    {
        if (scrolling || !dragging) return;
        dragging = false;
        surface.ReleaseMouseCapture();
        current = CursorPoint();
        if (dragHandle == -99)
        {
            if ((current - start).Length < 4 && !snap.IsEmpty) Selection = snap;
            else Selection = SelectionGeometry.Drag(start, current, Desktop.Width, Desktop.Height);
            if (Selection.Width < 2 || Selection.Height < 2) Selection = default;
        }
        else if (dragHandle == -100)
        {
            current = WithinSelection(current);
            PaintCommand command = Preview ?? new PaintCommand(Tool, start, current, InkColor, InkThickness,
                Tool is AnnotationTool.Pen or AnnotationTool.Marker ? penPoints.ToArray() : null,
                Opacity: Tool == AnnotationTool.Marker ? MarkerOpacity : 1, TextStyle: Tool == AnnotationTool.Text ? TextStyle : 0);
            if (Tool is AnnotationTool.Pixelate or AnnotationTool.Blur)
            {
                PixelRect effectBounds = SelectionGeometry.Drag(start, current, Desktop.Width, Desktop.Height).Intersect(Selection);
                if (!effectBounds.IsEmpty) Document.Add(Document.Effect(Tool, effectBounds, InkColor, InkThickness));
            }
            else if (Tool is AnnotationTool.Pen or AnnotationTool.Marker || (current - start).Length >= 1) Document.Add(command);
        }
        Preview = null;
        Update();
        e.Handled = true;
        Update();
    }

    /// <summary>Feishu-style crop: bake the current region and let the workflow reopen the
    /// overlay shrunk to that region so the analyst can fine-tune the crop in place.</summary>
    public void BeginCrop()
    {
        if (Selection.IsEmpty || scrolling || finishing || regionOnly) return;
        PixelRect absolute = AbsoluteSelection;
        if (absolute.Width < 8 || absolute.Height < 8) return;
        if (Document.Export(Selection) is not { } baked) return;
        CroppedFrame = ToFrame(baked);
        CropRegion = absolute;
        Result = null;
        Close();
    }

    /// <summary>Copies a bitmap into the BGRA32 buffer the overlay works with.</summary>
    private static PixelFrame ToFrame(BitmapSource source)
    {
        BitmapSource bgra = source.Format == PixelFormats.Bgra32 ? source
            : new FormatConvertedBitmap(source, PixelFormats.Bgra32, null, 0);
        int stride = checked(bgra.PixelWidth * 4);
        byte[] pixels = new byte[checked(stride * bgra.PixelHeight)];
        bgra.CopyPixels(pixels, stride, 0);
        return new PixelFrame(bgra.PixelWidth, bgra.PixelHeight, pixels);
    }

    public void SetTool(AnnotationTool tool)
    {
        Tool = tool;
        surface.Cursor = tool == AnnotationTool.Text ? Cursors.IBeam : Cursors.Cross;
        Preview = null;
        Update();
    }

    public void Undo() { Document.Undo(); Update(); }
    public void Redo() { Document.Redo(); Update(); }
    public void Cancel()
    {
        if (finishing) return;
        if (scrolling) { scrollSession?.Close(); return; }
        Close();
    }
    public void Record()
    {
        if (Selection.IsEmpty || dragging || finishing || scrolling) return;
        Result = new ScreenshotResult(AbsoluteSelection, Document.Export(Selection), RecordRequested: true, Mode: CaptureMode.Recording);
        Close();
    }
    public void ChooseMode(CaptureMode mode)
    {
        if (scrolling || dragging || finishing) return;
        modeBar.Select(mode);
        scrollArmed = false;
        hintNote = null;
        if (Selection.IsEmpty)
        {
            if (regionOnly) { Update(); return; }
            // Only scrolling capture can wait for a region; every other function needs one first.
            if (mode != CaptureMode.Scrolling) { hintNote = "先框选区域，再使用此功能"; Update(); return; }
            scrollArmed = true;
            hintNote = "先框选要滚动截取的区域，松开后自动开始滚动截图";
            Update();
            return;
        }
        if (mode == CaptureMode.Screenshot && !regionOnly)
        {
            SetTool(AnnotationTool.Select); return;
        }
        if (regionOnly) { RegionResult = AbsoluteSelection; Close(); return; }
        if (mode == CaptureMode.Scrolling)
        {
            Update();
            if (!regionOnly && !Selection.IsEmpty) scrollStartBar.Visibility = Visibility.Visible;
            return;
        }
        if (mode == CaptureMode.Text) { _ = RecognizeTextAsync(); return; }
        Result = new ScreenshotResult(AbsoluteSelection, Document.Export(Selection), RecordRequested: mode == CaptureMode.Recording, Mode: mode);
        Close();
    }

    /// <summary>Toolbar 自动滚动 button: starts scrolling capture for the current region.</summary>
    public void BeginAutoScroll()
    {
        if (Selection.IsEmpty || scrolling || finishing || regionOnly) return;
        EnterScrollingMode();
    }

    /// <summary>Toolbar OCR button: same inline recognition as the text mode.</summary>
    public void BeginOcr()
    {
        if (Selection.IsEmpty || scrolling || finishing || regionOnly) return;
        _ = RecognizeTextAsync();
    }

    /// <summary>Runs OCR inside the overlay and shows the result in the inline panel (Feishu parity).</summary>
    private async Task RecognizeTextAsync()
    {
        ocrCancellation?.Cancel();
        ocrCancellation?.Dispose();
        ocrCancellation = new CancellationTokenSource();
        CancellationToken token = ocrCancellation.Token;
        textPanel.Visibility = Visibility.Visible;
        textPanel.ShowBusy();
        hintNote = null;
        Update();
        try
        {
            BitmapSource image = Document.Export(Selection);
            string text = await OcrService.ExtractTextAsync(image, token);
            if (token.IsCancellationRequested || !IsVisible) return;
            textPanel.ShowText(text);
        }
        catch (OperationCanceledException) { return; }
        catch (Exception error)
        {
            if (token.IsCancellationRequested || !IsVisible) return;
            textPanel.ShowFailure(error.Message);
        }
        Update();
    }

    /// <summary>Starts region recording with the format chosen inside the overlay (Feishu parity).</summary>
    private void StartRecordingFromPanel(Cap.Core.RecordingFormat format)
    {
        if (Selection.IsEmpty || dragging || finishing || scrolling || regionOnly) return;
        Result = new ScreenshotResult(AbsoluteSelection, Document.Export(Selection), RecordRequested: true, Mode: CaptureMode.Recording)
        {
            RecordingFormat = format
        };
        Close();
    }

    /// <summary>Closes the inline OCR panel and returns to the selection overlay.</summary>
    private void CloseTextPanel()
    {
        ocrCancellation?.Cancel();
        textPanel.Visibility = Visibility.Collapsed;
        Update();
    }

    /// <summary>Scrolling capture waits for the explicit start bar, matching Feishu.</summary>
    private void StartArmedScrolling()
    {
        if (!scrollArmed || scrolling) return;
        scrollArmed = false;
        if (Selection.IsEmpty) hintNote = "先框选要滚动截取的区域，框选后点击「开始滚动截图」";
        else if (Selection.Width < 64 || Selection.Height < 80) hintNote = "滚动截图区域至少需要 64 × 80 像素，请重新框选";
        else hintNote = null;
        Update();
    }

    /// <summary>Shows the magnifier/colour readout next to the pointer, as Feishu does.</summary>
    private void UpdateColorPicker(Point physicalPoint)
    {
        if (colorPicker.Visibility != Visibility.Visible || ActualWidth <= 0 || ActualHeight <= 0) return;
        double sx = ActualWidth / Desktop.Width, sy = ActualHeight / Desktop.Height;
        var screen = new PixelRect(Desktop.X + (int)physicalPoint.X, Desktop.Y + (int)physicalPoint.Y, 1, 1);
        colorPicker.Update(screen, new Point(Desktop.X, Desktop.Y));
        colorPicker.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
        double width = colorPicker.DesiredSize.Width, height = colorPicker.DesiredSize.Height;
        double x = physicalPoint.X * sx - width - 10;
        double y = physicalPoint.Y * sy + 11;
        if (x < 4) x = physicalPoint.X * sx + 18;
        if (y + height > ActualHeight - 4) y = Math.Max(4, physicalPoint.Y * sy - height - 11);
        Canvas.SetLeft(colorPicker, Math.Clamp(x, 0, Math.Max(0, ActualWidth - width)));
        Canvas.SetTop(colorPicker, Math.Clamp(y, 0, Math.Max(0, ActualHeight - height)));
    }

    /// <summary>Compact help affordance for scrolling capture (Feishu exposes the same "帮助" item).</summary>
    private void ToggleScrollHelp()
    {
        if (scrollHelp is null)
        {
            scrollHelp = new Border
            {
                Background = new SolidColorBrush(Color.FromArgb(238, 41, 41, 41)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(255, 58, 58, 58)),
                BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(8),
                Padding = new Thickness(12, 9, 12, 10), MaxWidth = 320,
                Visibility = Visibility.Collapsed,
                Child = new TextBlock
                {
                    Text = "滚动截图：框选区域后点击「开始滚动截图」，在选区里滚动鼠标滚轮或按方向键逐屏拼接，完成后保存长图。Esc 取消。",
                    Foreground = Brushes.White, FontSize = 12.5, TextWrapping = TextWrapping.Wrap
                }
            };
            canvas.Children.Add(scrollHelp);
        }
        scrollHelp.Visibility = scrollHelp.Visibility == Visibility.Visible ? Visibility.Collapsed : Visibility.Visible;
        Update();
    }

    /// <summary>Keeps the overlay open and lets the target page scroll inside the selection instead of ending the screenshot.</summary>
    private async void EnterScrollingMode()
    {
        if (scrolling || finishing || Selection.IsEmpty) return;
        if (Selection.Width < 64 || Selection.Height < 80)
        {
            hintNote = "滚动截图区域至少需要 64 × 80 像素，请重新框选";
            Update();
            return;
        }
        scrolling = true;
        scrollArmed = false;
        hintNote = null;
        Tool = AnnotationTool.Select;
        Preview = null;
        Point? barPosition = ModeBarScreenPosition();
        ApplyCutout();
        Update();
        var session = new ScrollingCaptureWindow(AbsoluteSelection, dark, barPosition, this);
        scrollSession = session;
        BitmapSource? stitched = null;
        try
        {
            session.Show();
            stitched = await session.Completion;
        }
        catch (Exception error)
        {
            scrollSession = null;
            try { session.Close(); } catch (InvalidOperationException) { }
            if (IsVisible) ExitScrollingMode();
            hintNote = $"无法开始滚动截图：{error.Message}";
            Update();
            return;
        }
        scrollSession = null;
        if (!IsVisible || finishing) return;
        ExitScrollingMode();
        if (stitched is null) return;
        Result = new ScreenshotResult(AbsoluteSelection, stitched, Mode: CaptureMode.Scrolling) { ScrollImage = stitched };
        Close();
    }

    private void ExitScrollingMode()
    {
        scrolling = false;
        scrollArmed = false;
        if (!regionOnly) modeBar.Select(CaptureMode.Screenshot);
        ApplyCutout();
        Update();
        if (!IsVisible) return;
        Activate();
        surface.Focus();
    }

    /// <summary>The mode bar's top-left corner in absolute physical screen pixels.</summary>
    private Point? ModeBarScreenPosition()
    {
        if (ActualWidth <= 0 || ActualHeight <= 0) return null;
        double left = Canvas.GetLeft(modeBar), top = Canvas.GetTop(modeBar);
        if (!double.IsFinite(left) || !double.IsFinite(top)) return null;
        return new Point(Desktop.X + left * Desktop.Width / ActualWidth, Desktop.Y + top * Desktop.Height / ActualHeight);
    }

    /// <summary>Clips the overlay away from the scrolling region so the page stays live and receives the real wheel input.</summary>
    private void ApplyCutout()
    {
        nint handle = new WindowInteropHelper(this).Handle;
        if (handle == 0) return;
        if (!scrolling || Selection.IsEmpty) { SetWindowRgn(handle, 0, true); return; }
        if (!GetClientRect(handle, out NativeClientRect client)) return;
        nint region = CreateRectRgn(0, 0, client.Right, client.Bottom);
        if (region == 0) return;
        int left = Math.Clamp(Selection.X - CutoutMargin, 0, client.Right);
        int top = Math.Clamp(Selection.Y - CutoutMargin, 0, client.Bottom);
        int right = Math.Clamp(Selection.Right + CutoutMargin, 0, client.Right);
        int bottom = Math.Clamp(Selection.Bottom + CutoutMargin, 0, client.Bottom);
        if (right > left && bottom > top)
        {
            nint hole = CreateRectRgn(left, top, right, bottom);
            if (hole != 0)
            {
                CombineRgn(region, region, hole, RegionDifference);
                DeleteObject(hole);
            }
        }
        if (SetWindowRgn(handle, region, true) == 0) DeleteObject(region);
    }

    private void Update()
    {
        surface.InvalidateVisual();
        toolbar.Update();
        bool hasSelection = !Selection.IsEmpty && !dragging;
        toolbar.Visibility = hasSelection && !scrolling && !regionOnly && modeBar.SelectedMode == CaptureMode.Screenshot ? Visibility.Visible : Visibility.Collapsed;
        // The picker stays available before a selection so scrolling capture can be armed first.
        // Feishu hides the function bar while the crop stage owns a shrunken window.
        modeBar.Visibility = scrolling || preselectAll ? Visibility.Collapsed : Visibility.Visible;
        colorPicker.Visibility = !scrolling && !finishing && modeBar.SelectedMode != CaptureMode.Recording &&
            (Selection.IsEmpty || (dragging && Tool == AnnotationTool.Select)) ? Visibility.Visible : Visibility.Collapsed;
        if (scrollStartBar is not null)
            scrollStartBar.Visibility = !scrolling && !regionOnly && !dragging && modeBar.SelectedMode == CaptureMode.Scrolling && !Selection.IsEmpty
                ? Visibility.Visible : Visibility.Collapsed;
        if (scrollHelp is not null && scrollStartBar?.Visibility != Visibility.Visible) scrollHelp.Visibility = Visibility.Collapsed;
        hint.Text = hintNote ?? DefaultHint;
        hint.Visibility = !scrolling && (Selection.IsEmpty || hintNote is not null) && !dragging ? Visibility.Visible : Visibility.Collapsed;
        Canvas.SetLeft(hint, 24);
        Canvas.SetTop(hint, Math.Max(12, ActualHeight - 62));
        if (scrolling || dragging || ActualWidth <= 0 || ActualHeight <= 0) return;
        double sx = ActualWidth / Desktop.Width, sy = ActualHeight / Desktop.Height;
        // Keep both bars reachable on the monitor containing the selection's lower-right corner.
        Point cursor = CursorPoint();
        Point anchor = Selection.IsEmpty
            ? new Point(Desktop.X + (int)cursor.X, Desktop.Y + (int)cursor.Y)
            : new Point(Desktop.X + Selection.Right - 1, Desktop.Y + Selection.Bottom - 1);
        PixelRect monitor = NativeDesktop.MonitorAt(anchor).Intersect(Desktop);
        if (monitor.IsEmpty) monitor = Desktop;
        double topLimit = (monitor.Y - Desktop.Y) * sy + 8;
        double bottomLimit = (monitor.Bottom - Desktop.Y) * sy - 8;
        Rect PlaceBar(FrameworkElement bar, bool above, PixelRect destination, Point? position = null)
        {
            double leftLimit = (destination.X - Desktop.X) * sx + 8;
            double rightLimit = (destination.Right - Desktop.X) * sx - 8;
            double barTopLimit = (destination.Y - Desktop.Y) * sy + 8;
            double barBottomLimit = (destination.Bottom - Desktop.Y) * sy - 8;
            bar.MaxWidth = Math.Max(120, Math.Min(960, rightLimit - leftLimit));
            bar.Measure(new Size(bar.MaxWidth, double.PositiveInfinity));
            Size size = bar.DesiredSize;
            double x = Math.Clamp(position?.X * sx ?? Selection.Right * sx - size.Width, leftLimit, Math.Max(leftLimit, rightLimit - size.Width));
            double y = position?.Y * sy ?? (above ? Selection.Y * sy - size.Height - 34 : Selection.Bottom * sy + 10);
            if (position == null)
            {
                if (above && y < barTopLimit) y = Selection.Y * sy + 10;
                if (!above && y + size.Height > barBottomLimit) y = Selection.Bottom * sy - size.Height - 10;
            }
            y = Math.Clamp(y, barTopLimit, Math.Max(barTopLimit, barBottomLimit - size.Height));
            Canvas.SetLeft(bar, x); Canvas.SetTop(bar, y);
            return new Rect(x, y, size.Width, size.Height);
        }
        // Without a selection the picker waits at the working area's lower-right corner instead of over the drag area.
        PixelRect placementArea = modeBarPosition.HasValue ? modeBarMonitor
            : Selection.IsEmpty ? NativeDesktop.WorkAreaAt(anchor).Intersect(Desktop) : monitor;
        if (placementArea.IsEmpty) placementArea = monitor;
        Point? placement = modeBarPosition ?? (Selection.IsEmpty ? new Point(placementArea.Right - 1, placementArea.Bottom - 1) : null);
        Rect modeBounds = PlaceBar(modeBar, true, placementArea, placement);
        // Feishu keeps the function bar parked at the top centre of the monitor,
        // even after a selection exists; only a manual drag moves it.
        if (!modeBarPosition.HasValue && ActualWidth > 0)
        {
            double centered = Math.Clamp((ActualWidth - modeBounds.Width) / 2, 8, Math.Max(8, ActualWidth - modeBounds.Width - 8));
            Canvas.SetLeft(modeBar, centered);
            Canvas.SetTop(modeBar, 5);
            modeBounds = new Rect(centered, 5, modeBounds.Width, modeBounds.Height);
        }
        recordingPanel.Visibility = !scrolling && !regionOnly && !dragging && modeBar.SelectedMode == CaptureMode.Recording && !Selection.IsEmpty
            ? Visibility.Visible : Visibility.Collapsed;
        if (scrollStartBar!.Visibility == Visibility.Visible)
        {
            scrollStartBar.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            Size startSize = scrollStartBar.DesiredSize;
            double barX = Math.Clamp(Selection.X * sx + 13 * sx, 8, Math.Max(8, ActualWidth - startSize.Width - 8));
            double barY = Selection.Bottom * sy + 15 * sy;
            if (barY + startSize.Height > ActualHeight - 8) barY = Math.Max(8, Selection.Y * sy - startSize.Height - 15 * sy);
            Canvas.SetLeft(scrollStartBar, barX);
            Canvas.SetTop(scrollStartBar, Math.Clamp(barY, 8, Math.Max(8, ActualHeight - startSize.Height - 8)));
            if (scrollHelp is not null && scrollHelp.Visibility == Visibility.Visible)
            {
                scrollHelp.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
                Size helpSize = scrollHelp.DesiredSize;
                Canvas.SetLeft(scrollHelp, Math.Clamp(barX, 8, Math.Max(8, ActualWidth - helpSize.Width - 8)));
                Canvas.SetTop(scrollHelp, Math.Max(8, Canvas.GetTop(scrollStartBar) - helpSize.Height - 8));
            }
        }
        if (recordingPanel.Visibility == Visibility.Visible)
        {
            recordingPanel.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            Size recordSize = recordingPanel.DesiredSize;
            double recordX = Math.Clamp(Selection.X * sx + (Selection.Width * sx - recordSize.Width) / 2, 8, Math.Max(8, ActualWidth - recordSize.Width - 8));
            double recordY = Math.Clamp(Selection.Bottom * sy - recordSize.Height - 12 * sy, 8, Math.Max(8, ActualHeight - recordSize.Height - 8));
            Canvas.SetLeft(recordingPanel, recordX);
            Canvas.SetTop(recordingPanel, recordY);
        }
        if (textPanel.Visibility == Visibility.Visible)
        {
            textPanel.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            Size panelSize = textPanel.DesiredSize;
            double panelX = Math.Clamp(Selection.X * sx + 16 * sx, 8, Math.Max(8, ActualWidth - panelSize.Width - 8));
            double panelY = Math.Clamp(Selection.Y * sy + Selection.Height * sy * .32, 8, Math.Max(8, ActualHeight - panelSize.Height - 8));
            Canvas.SetLeft(textPanel, panelX);
            Canvas.SetTop(textPanel, panelY);
        }
        if (toolbar.Visibility == Visibility.Visible)
        {            Rect toolsBounds = toolbarPosition.HasValue && !toolbarMonitor.IsEmpty
                ? PlaceBar(toolbar, false, toolbarMonitor, toolbarPosition)
                : PlaceBar(toolbar, false, monitor);
            if (modeBounds.IntersectsWith(toolsBounds))
            {
                double y = modeBounds.Bottom + 8;
                if (y + toolsBounds.Height > bottomLimit) y = modeBounds.Top - toolsBounds.Height - 8;
                Canvas.SetTop(toolbar, Math.Clamp(y, topLimit, Math.Max(topLimit, bottomLimit - toolsBounds.Height)));
            }
        }
    }

    private void OnKey(object sender, KeyEventArgs e)
        => HandleKey(e, Keyboard.Modifiers);

    private void HandleKey(KeyEventArgs e, ModifierKeys modifiers)
    {
        if (finishing) { e.Handled = true; return; }
        Key key = e.Key == Key.System ? e.SystemKey : e.Key;
        if (scrolling)
        {
            e.Handled = true;
            if (key == Key.Escape) scrollSession?.Close();
            else if (key == Key.Enter) scrollSession?.RequestFinish();
            return;
        }
        bool control = modifiers.HasFlag(ModifierKeys.Control);
        if (key == Key.Escape)
        {
            e.Handled = true;
            if (textPanel.Visibility == Visibility.Visible) CloseTextPanel();
            else Cancel();
        }
        else if (!Selection.IsEmpty && modifiers == ModifierKeys.Alt && key is Key.Left or Key.Right) { modeBar.Move(key == Key.Left ? -1 : 1); SetTool(AnnotationTool.Select); e.Handled = true; }
        else if ((key == Key.LeftShift || key == Key.RightShift || key == Key.LeftCtrl || key == Key.RightCtrl) && colorPicker.Visibility == Visibility.Visible)
        {
            e.Handled = true;
            colorPicker.ToggleFormat();
        }
        else if (control && key == Key.C)
        {
            e.Handled = true;
            if (textPanel.Visibility == Visibility.Visible && textPanel.HasText) textPanel.CopyAll();
            else if (colorPicker.Visibility == Visibility.Visible && Selection.IsEmpty) colorPicker.CopyValue();
            else Copy();
        }
        else if (key == Key.Enter) { e.Handled = true; Complete(); }
        else if (!regionOnly && control && key == Key.S) { e.Handled = true; Save(); }
        else if (!regionOnly && control && key == Key.Z)
        {
            e.Handled = true;
            if (modifiers.HasFlag(ModifierKeys.Shift)) Redo(); else Undo();
        }
        else if (!regionOnly && control && key == Key.Y) { e.Handled = true; Redo(); }
        else if (!Selection.IsEmpty && Tool == AnnotationTool.Select && key is Key.Left or Key.Right or Key.Up or Key.Down)
        {
            int step = modifiers.HasFlag(ModifierKeys.Shift) ? 10 : 1;
            Point delta = key switch { Key.Left => new(-step, 0), Key.Right => new(step, 0), Key.Up => new(0, -step), _ => new(0, step) };
            Selection = SelectionGeometry.Adjust(Selection, new Point(), delta, -1, Desktop.Width, Desktop.Height);
            e.Handled = true;
            Update();
        }
    }

    private PixelRect AbsoluteSelection => Selection with { X = Selection.X + Desktop.X, Y = Selection.Y + Desktop.Y };

    private const int CutoutMargin = 2;
    private const int RegionDifference = 4;

    [StructLayout(LayoutKind.Sequential)] private struct NativeClientRect { public int Left; public int Top; public int Right; public int Bottom; }
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetClientRect(nint window, out NativeClientRect rect);
    [DllImport("user32.dll")] private static extern int SetWindowRgn(nint window, nint region, [MarshalAs(UnmanagedType.Bool)] bool redraw);
    [DllImport("gdi32.dll")] private static extern nint CreateRectRgn(int left, int top, int right, int bottom);
    [DllImport("gdi32.dll")] private static extern int CombineRgn(nint destination, nint source1, nint source2, int mode);
    [DllImport("gdi32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool DeleteObject(nint handle);

    public void Complete()
    {
        if (Selection.IsEmpty || dragging || finishing || scrolling) return;
        if (modeBar.SelectedMode != CaptureMode.Screenshot) { ChooseMode(modeBar.SelectedMode); return; }
        Copy();
    }

    public async void Copy()
    {
        if (Selection.IsEmpty || dragging || finishing || scrolling) return;
        if (regionOnly) { RegionResult = AbsoluteSelection; Close(); return; }
        finishing = true;
        try
        {
            BitmapSource image = Document.Export(Selection);
            ExternalException? clipboardError = null;
            for (int attempt = 0; attempt < 4; attempt++)
            {
                try { Clipboard.SetImage(image); clipboardError = null; break; }
                catch (ExternalException ex) { clipboardError = ex; await Task.Delay(80); }
            }
            if (clipboardError is not null) throw clipboardError;
            Result = new ScreenshotResult(AbsoluteSelection, image, Copied: true);
            Close();
        }
        catch (Exception ex) when (ex is ExternalException or ArgumentException or InvalidOperationException or OutOfMemoryException)
        {
            MessageBox.Show(this, $"复制失败，请重试或保存图片。\n{ex.Message}", "Cap", MessageBoxButton.OK, MessageBoxImage.Warning);
        }
        finally { finishing = false; }
    }

    public void Save()
    {
        if (Selection.IsEmpty || dragging || finishing || scrolling) return;
        try
        {
            Directory.CreateDirectory(saveDirectory);
            var dialog = new SaveFileDialog
            {
                Title = "保存截图", InitialDirectory = saveDirectory, FileName = $"Cap_{DateTime.Now:yyyyMMdd_HHmmss}.png",
                Filter = "PNG 图片 (*.png)|*.png|JPEG 图片 (*.jpg;*.jpeg)|*.jpg;*.jpeg", DefaultExt = ".png", AddExtension = true, OverwritePrompt = true
            };
            if (dialog.ShowDialog(this) != true) return;
            BitmapSource image = Document.Export(Selection);
            ScreenshotExport.Save(image, dialog.FileName);
            Result = new ScreenshotResult(AbsoluteSelection, image, dialog.FileName);
            Close();
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or ArgumentException or InvalidOperationException or NotSupportedException or OutOfMemoryException)
        {
            MessageBox.Show(this, $"保存失败：{ex.Message}", "Cap", MessageBoxButton.OK, MessageBoxImage.Warning);
        }
    }

    public void Pin()
    {
        if (Selection.IsEmpty || dragging || finishing || scrolling) return;
        try
        {
            BitmapSource image = Document.Export(Selection);
            var pin = new PinWindow(image, AbsoluteSelection);
            pin.Show();
            Result = new ScreenshotResult(AbsoluteSelection, image, Pinned: true);
            Close();
        }
        catch (Exception ex) when (ex is InvalidOperationException or OutOfMemoryException or ArgumentException)
        {
            MessageBox.Show(this, $"贴图失败：{ex.Message}", "Cap", MessageBoxButton.OK, MessageBoxImage.Warning);
        }
    }
}
