using Cap.Core;
using Windows.Graphics;
using Windows.Graphics.Capture;
using Windows.Graphics.DirectX;

namespace Cap.Capture;

internal sealed class GraphicsCaptureSource : ICaptureSource
{
    private readonly object gate = new();
    private readonly object signalGate = new();
    private readonly AutoResetEvent arrived = new(false);
    private readonly GraphicsCaptureItem item;
    private readonly D3D11Bridge bridge;
    private readonly Direct3D11CaptureFramePool pool;
    private readonly GraphicsCaptureSession session;
    private readonly int offsetX, offsetY;
    private readonly nint windowHandle;
    private readonly PixelRect requestedBounds;
    private SizeInt32 poolSize;
    private PixelFrame? previousFrame;
    private byte[] previousPixels = Array.Empty<byte>();
    private bool closed, disposed;
    public PixelRect Bounds
    {
        get
        {
            if (windowHandle == 0) return requestedBounds;
            PixelRect current = CaptureService.GetWindowBounds(windowHandle);
            return requestedBounds with { X = current.X, Y = current.Y };
        }
    }
    internal bool TargetClosed => Volatile.Read(ref closed);
    public string BackendName => "Windows Graphics Capture";

    internal static GraphicsCaptureSource? TryCreate(CaptureRequest request)
    {
        if (!GraphicsCaptureSession.IsSupported()) return null;
        using var dpi = new NativeMethods.DpiScope();
        if (request.WindowHandle != 0)
        {
            if (!NativeMethods.IsWindow(request.WindowHandle)) throw new CaptureTargetClosedException();
            return new(request.Bounds, D3D11Bridge.CreateItem(request.WindowHandle, true), 0, 0, request.WindowHandle);
        }
        foreach (var monitor in CaptureService.GetMonitorHandles())
        {
            if (request.Bounds.Intersect(monitor.Bounds) == request.Bounds)
                return new(request.Bounds, D3D11Bridge.CreateItem(monitor.Handle, false),
                    request.Bounds.X - monitor.Bounds.X, request.Bounds.Y - monitor.Bounds.Y, 0);
        }
        return null;
    }

    private GraphicsCaptureSource(PixelRect bounds, GraphicsCaptureItem captureItem, int x, int y, nint window)
    {
        requestedBounds = bounds;
        windowHandle = window;
        item = captureItem;
        offsetX = x;
        offsetY = y;
        try
        {
            bridge = new D3D11Bridge();
            poolSize = item.Size;
            if (poolSize.Width <= 0 || poolSize.Height <= 0) throw new InvalidOperationException("The capture target has no drawable surface.");
            pool = Direct3D11CaptureFramePool.CreateFreeThreaded(bridge.WinRtDevice,
                DirectXPixelFormat.B8G8R8A8UIntNormalized, 2, poolSize);
            session = pool.CreateCaptureSession(item);
            session.IsCursorCaptureEnabled = false;
            pool.FrameArrived += OnFrameArrived;
            item.Closed += OnClosed;
            session.StartCapture();
        }
        catch
        {
            Dispose();
            throw;
        }
    }

    private void OnFrameArrived(Direct3D11CaptureFramePool sender, object args)
    {
        lock (signalGate)
        {
            if (!disposed) arrived.Set();
        }
    }

    private void OnClosed(GraphicsCaptureItem sender, object args)
    {
        Volatile.Write(ref closed, true);
        lock (signalGate)
        {
            if (!disposed) arrived.Set();
        }
    }

    public PixelFrame Capture()
    {
        lock (gate)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            if (Volatile.Read(ref closed)) throw new CaptureTargetClosedException();
            Direct3D11CaptureFrame? frame = pool.TryGetNextFrame();
            if (frame is null && previousFrame is null)
            {
                arrived.WaitOne(1500);
                if (Volatile.Read(ref closed)) throw new CaptureTargetClosedException();
                frame = pool.TryGetNextFrame();
            }
            if (frame is null)
                // The recording loop owns frame pacing; overlays must advance even on a static target.
                return previousFrame is not null ? Clone(previousFrame) : throw new TimeoutException("Windows Graphics Capture did not produce an initial frame.");
            SizeInt32 content;
            PixelFrame output;
            try
            {
                // Drain old frames so an overloaded encoder records the latest desktop state.
                Direct3D11CaptureFrame? newer;
                while ((newer = pool.TryGetNextFrame()) is not null)
                {
                    frame.Dispose();
                    frame = newer;
                }
                content = frame.ContentSize;
                using var surface = frame.Surface;
                // Try a non-blocking GPU readback first: waiting for the copy to finish
                // on the capture thread was the dominant cost at 1440p60.
                byte[]? pixels = bridge.ReadPixels(surface, content.Width, content.Height, out int width, out int height, nonBlocking: true);
                if (pixels is null)
                {
                    if (previousFrame is not null) return Clone(previousFrame);
                    pixels = bridge.ReadPixels(surface, content.Width, content.Height, out width, out height);
                    if (pixels is null) throw new TimeoutException("Windows Graphics Capture readback did not complete.");
                }
                output = Crop(pixels, width, height);
                StorePrevious(output);
            }
            finally
            {
                frame.Dispose();
            }
            if (content.Width > 0 && content.Height > 0 && (content.Width != poolSize.Width || content.Height != poolSize.Height))
            {
                poolSize = content;
                pool.Recreate(bridge.WinRtDevice, DirectXPixelFormat.B8G8R8A8UIntNormalized, 2, poolSize);
            }
            return output;
        }
    }

    private PixelFrame Crop(byte[] source, int width, int height)
    {
        // Full-monitor captures need no copy at all: the readback buffer is already the frame.
        if (offsetX == 0 && offsetY == 0 && width == requestedBounds.Width && height == requestedBounds.Height)
            return new(requestedBounds.Width, requestedBounds.Height, source);
        byte[] output = new byte[checked(requestedBounds.Width * requestedBounds.Height * 4)];
        int copyWidth = Math.Min(requestedBounds.Width, Math.Max(0, width - offsetX));
        int copyHeight = Math.Min(requestedBounds.Height, Math.Max(0, height - offsetY));
        int sourceStride = checked(width * 4), targetStride = checked(requestedBounds.Width * 4);
        for (int row = 0; row < copyHeight; row++)
            Buffer.BlockCopy(source, (row + offsetY) * sourceStride + offsetX * 4, output, row * targetStride, copyWidth * 4);
        for (int alpha = 3; alpha < output.Length; alpha += 4) output[alpha] = 255;
        return new(requestedBounds.Width, requestedBounds.Height, output);
    }

    private static PixelFrame Clone(PixelFrame frame) => new(frame.Width, frame.Height, (byte[])frame.Pixels.Clone());

    /// <summary>Keeps the last clean frame (before overlays) in a reused buffer.</summary>
    private void StorePrevious(PixelFrame frame)
    {
        if (previousPixels.Length != frame.Pixels.Length) previousPixels = new byte[frame.Pixels.Length];
        Buffer.BlockCopy(frame.Pixels, 0, previousPixels, 0, frame.Pixels.Length);
        previousFrame = new PixelFrame(frame.Width, frame.Height, previousPixels);
    }

    public void Dispose()
    {
        lock (gate)
        {
            lock (signalGate)
            {
                if (disposed) return;
                disposed = true;
            }
            item.Closed -= OnClosed;
            if (pool is not null) pool.FrameArrived -= OnFrameArrived;
            session?.Dispose();
            pool?.Dispose();
            bridge?.Dispose();
            arrived.Dispose();
        }
    }
}
