using System;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Cap.Capture;
using Cap.Core;

namespace Cap.Recording;

/// <summary>One background capture/encode pipeline. Events are raised on its worker thread.</summary>
public sealed class RecordingSession : IDisposable
{
    private readonly RecordingOptions options;
    private readonly object sync = new();
    private readonly AutoResetEvent changed = new(false);
    private readonly Stopwatch active = new();
    private readonly TaskCompletionSource<RecordingResult> completion = new(TaskCreationOptions.RunContinuationsAsynchronously);
    private RecordingState state;
    private bool disposed, signalDisposed;
    private int workerThread;
    private readonly int width, height;

    public RecordingState State { get { lock (sync) return state; } }
    public Task<RecordingResult> Completion => completion.Task;
    public event EventHandler<RecordingProgress>? Progress;
    public event EventHandler<Exception>? Failed;

    public RecordingSession(RecordingOptions options)
    {
        ArgumentNullException.ThrowIfNull(options);
        Validate(options);
        this.options = options with { OutputPath = Path.GetFullPath(options.OutputPath) };
        (width, height) = ResolveDimensions(options);
    }

    public void Start()
    {
        lock (sync)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            if (state != RecordingState.Idle) throw new InvalidOperationException("A recording session can only be started once.");
            if (File.Exists(options.OutputPath)) throw new IOException("Output already exists. Choose a new file name.");
            state = RecordingState.Recording;
            Task.Factory.StartNew(Run, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
        }
    }

    public void Pause()
    {
        lock (sync)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            if (state != RecordingState.Recording) return;
            active.Stop(); state = RecordingState.Paused; changed.Set();
        }
    }

    public void Resume()
    {
        lock (sync)
        {
            ObjectDisposedException.ThrowIf(disposed, this);
            if (state != RecordingState.Paused) return;
            active.Start(); state = RecordingState.Recording; changed.Set();
        }
    }

    public Task<RecordingResult> StopAsync()
    {
        lock (sync)
        {
            if (state == RecordingState.Idle) return Task.FromException<RecordingResult>(new InvalidOperationException("The recording has not started."));
            if (state is RecordingState.Recording or RecordingState.Paused)
            {
                active.Stop(); state = RecordingState.Stopping; changed.Set();
            }
            return completion.Task;
        }
    }

    private void Run()
    {
        workerThread = Environment.CurrentManagedThreadId;
        string journalPath = options.OutputPath + ".capjournal";
        string partialPath = options.OutputPath + ".partial." + (options.Format == RecordingFormat.Mp4 ? "mp4" : "gif");
        try
        {
            string directory = Path.GetDirectoryName(options.OutputPath)!;
            Directory.CreateDirectory(directory);
            FrameJournal.CheckFreeSpace(options.OutputPath);
            using (var source = CaptureService.CreateSource(new CaptureRequest(options.Bounds, options.WindowHandle,
                PreferGraphicsCapture: !UsesDirectRegionCapture(options))))
            using (var processor = new FrameProcessor(options))
            using (var journal = new FrameJournal(journalPath, new JournalHeader(1, options.OutputPath, DateTime.UtcNow, width, height, options.FramesPerSecond, options.Format)))
            using (var encoder = FrameEncoder.Create(partialPath, options.Format, width, height, options.FramesPerSecond))
            {
                lock (sync)
                {
                    active.Reset();
                    if (state == RecordingState.Recording) active.Start();
                }
                long nextSlot = 0, pendingSlot = 0, count = 0, dropped = 0, lastProgress = -250;
                // Optional per-stage profiling (CAP_RECORD_PROFILE=1) for 60 fps tuning.
                bool profile = Environment.GetEnvironmentVariable("CAP_RECORD_PROFILE") == "1";
                double captureTicks = 0, processTicks = 0, writeTicks = 0;
                // Encoding and journal writes overlap the next capture on their own thread;
                // serialising them was the remaining bottleneck at 1440p60.
                using var writeQueue = new BlockingCollection<WriteItem>(2);
                Exception? writeFailure = null;
                var writer = new Thread(() =>
                {
                    try
                    {
                        foreach (WriteItem item in writeQueue.GetConsumingEnumerable())
                        {
                            long started = profile ? Stopwatch.GetTimestamp() : 0;
                            WriteFrame(encoder, journal, item.Frame, item.Start, item.End);
                            if (profile) writeTicks += Stopwatch.GetTimestamp() - started;
                        }
                    }
                    catch (Exception error) { writeFailure = error; }
                })
                {
                    IsBackground = true,
                    Name = "Cap encoder",
                    Priority = ThreadPriority.AboveNormal
                };
                writer.Start();
                PixelFrame? pending = null;
                while (true)
                {
                    RecordingState current;
                    long elapsed;
                    lock (sync) { current = state; elapsed = active.Elapsed.Ticks; }
                    if (current == RecordingState.Stopping) break;
                    if (current == RecordingState.Paused)
                    {
                        if (options.WindowHandle != 0 && !IsWindow(options.WindowHandle)) break;
                        changed.WaitOne(options.WindowHandle == 0 ? Timeout.Infinite : 250);
                        continue;
                    }
                    long slot = elapsed * options.FramesPerSecond / TimeSpan.TicksPerSecond;
                    if (slot < nextSlot)
                    {
                        long remaining = SlotTime(nextSlot) - elapsed;
                        changed.WaitOne((int)Math.Clamp(remaining / TimeSpan.TicksPerMillisecond, 1, 50));
                        continue;
                    }
                    PixelFrame captured;
                    PixelRect capturedBounds;
                    long stageStart = profile ? Stopwatch.GetTimestamp() : 0;
                    try { captured = source.Capture(); capturedBounds = source.Bounds; }
                    catch (CaptureTargetClosedException) when (pending is not null) { break; }
                    if (profile) { captureTicks += Stopwatch.GetTimestamp() - stageStart; stageStart = Stopwatch.GetTimestamp(); }
                    PixelFrame frame = processor.Process(captured, capturedBounds, width, height);
                    if (profile) { processTicks += Stopwatch.GetTimestamp() - stageStart; stageStart = Stopwatch.GetTimestamp(); }
                    lock (sync)
                    {
                        if (state != RecordingState.Recording) continue;
                    }
                    if (pending is not null)
                    {
                        if (!writeQueue.TryAdd(new WriteItem(pending, pendingSlot, slot), 250))
                            dropped++;
                        dropped += Math.Max(0, slot - nextSlot);
                    }
                    else slot = 0;
                    pending = frame; pendingSlot = slot; nextSlot = slot + 1; count++;
                    long milliseconds = elapsed / TimeSpan.TicksPerMillisecond;
                    if (milliseconds - lastProgress >= 250)
                    {
                        RaiseProgress(new RecordingProgress(TimeSpan.FromTicks(elapsed), count, dropped, source.BackendName));
                        lastProgress = milliseconds;
                    }
                }
                if (pending is null)
                {
                    // Stop during initialization still produces a valid one-frame recording.
                    pending = processor.Process(source.Capture(), source.Bounds, width, height);
                    pendingSlot = 0; count = 1;
                }
                long finalSlot;
                lock (sync) finalSlot = Math.Max(pendingSlot + 1, (active.Elapsed.Ticks * options.FramesPerSecond + TimeSpan.TicksPerSecond - 1) / TimeSpan.TicksPerSecond);
                writeQueue.Add(new WriteItem(pending, pendingSlot, finalSlot));
                writeQueue.CompleteAdding();
                writer.Join(TimeSpan.FromSeconds(15));
                if (writeFailure is not null)
                    throw new InvalidOperationException("Recording encoder failed: " + writeFailure.Message, writeFailure);
                encoder.Complete();
                RaiseProgress(new RecordingProgress(TimeSpan.FromTicks(SlotTime(finalSlot)), count, dropped, source.BackendName));
                if (profile && count > 0)
                {
                    double toMs = 1000.0 / Stopwatch.Frequency / count;
                    Console.WriteLine($"[profile] frames={count} capture={captureTicks * toMs:F2}ms process={processTicks * toMs:F2}ms write={writeTicks * toMs:F2}ms");
                }
                finalResult = new RecordingResult(options.OutputPath, TimeSpan.FromTicks(SlotTime(finalSlot)), count);
            }
            File.Move(partialPath, options.OutputPath, false);
            try { File.Delete(journalPath); }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            { Trace.TraceWarning("Recording saved, but its recovery journal could not be deleted: {0}", error.Message); }
            lock (sync) { active.Stop(); state = RecordingState.Completed; }
            completion.TrySetResult(finalResult!);
        }
        catch (Exception error)
        {
            lock (sync) { active.Stop(); state = RecordingState.Failed; }
            completion.TrySetException(error);
            RaiseFailure(error);
        }
        finally
        {
            // Disposing from an event handler must not wait on the current worker thread.
            lock (sync) if (disposed) DisposeSignal();
        }
    }

    private readonly record struct WriteItem(PixelFrame Frame, long Start, long End);

    private RecordingResult? finalResult;
    private long SlotTime(long slot) => checked(slot * TimeSpan.TicksPerSecond / options.FramesPerSecond);
    private void WriteFrame(IFrameEncoder encoder, FrameJournal journal, PixelFrame frame, long start, long end)
    {
        long time = SlotTime(start), duration = SlotTime(end) - time;
        journal.Write(frame, time, duration);
        encoder.Write(frame, time, duration);
    }

    private void RaiseProgress(RecordingProgress progress)
    {
        try { Progress?.Invoke(this, progress); }
        catch (Exception error) { Trace.TraceError("Recording progress subscriber failed: {0}", error); }
    }
    private void RaiseFailure(Exception failure)
    {
        try { Failed?.Invoke(this, failure); }
        catch (Exception error) { Trace.TraceError("Recording failure subscriber failed: {0}", error); }
    }

    /// <summary>Partial-screen GIFs use direct region capture so the visible indicator matches the saved area.</summary>
    public static bool UsesDirectRegionCapture(RecordingOptions options)
    {
        ArgumentNullException.ThrowIfNull(options);
        if (options.Format != RecordingFormat.Gif || options.WindowHandle != 0) return false;
        foreach (PixelRect monitor in CaptureService.GetMonitors())
            if (options.Bounds == monitor) return false;
        return true;
    }

    internal static (int Width, int Height) ResolveDimensions(RecordingOptions options)
    {
        int w = options.OutputWidth, h = options.OutputHeight;
        if (w == 0 && h == 0) { w = options.Bounds.Width; h = options.Bounds.Height; }
        else if (h == 0) { w = Math.Min(w, options.Bounds.Width); h = Math.Max(1, (int)Math.Round((double)w * options.Bounds.Height / options.Bounds.Width)); }
        else if (w == 0) { h = Math.Min(h, options.Bounds.Height); w = Math.Max(1, (int)Math.Round((double)h * options.Bounds.Width / options.Bounds.Height)); }
        else
        {
            double scale = Math.Min(1, Math.Min((double)options.Bounds.Width / w, (double)options.Bounds.Height / h));
            w = Math.Max(1, (int)Math.Round(w * scale)); h = Math.Max(1, (int)Math.Round(h * scale));
        }
        if (options.Format == RecordingFormat.Mp4) { w = Math.Max(2, w & ~1); h = Math.Max(2, h & ~1); }
        if (w > 8192 || h > 8192 || (long)w * h > 33_554_432) throw new ArgumentException("Output resolution exceeds the supported limit (8192 per side, 32 million pixels).");
        return (w, h);
    }

    private static void Validate(RecordingOptions value)
    {
        if (value.Bounds.IsEmpty || value.Bounds.Width > 32768 || value.Bounds.Height > 32768) throw new ArgumentException("Capture bounds are invalid.");
        if (!Enum.IsDefined(value.Format)) throw new ArgumentException("Unsupported recording format.");
        if (value.Format == RecordingFormat.Mp4 && value.FramesPerSecond is not (15 or 24 or 30 or 60)) throw new ArgumentException("MP4 supports 15, 24, 30 or 60 FPS.");
        if (value.Format == RecordingFormat.Gif && value.FramesPerSecond is < 1 or > 60) throw new ArgumentException("GIF supports 1 through 60 FPS.");
        if (value.OutputWidth < 0 || value.OutputHeight < 0) throw new ArgumentException("Output dimensions cannot be negative.");
        if (value.CoordinateFontSize is < 8 or > 96) throw new ArgumentException("Coordinate font size must be 8 through 96.");
        if (value.CoordinateColor is null || value.CoordinateColor.Length != 7 || value.CoordinateColor[0] != '#' || !uint.TryParse(value.CoordinateColor.AsSpan(1), System.Globalization.NumberStyles.HexNumber, null, out _))
            throw new ArgumentException("Coordinate color must use #RRGGBB.");
        ArgumentException.ThrowIfNullOrWhiteSpace(value.OutputPath);
        string extension = Path.GetExtension(value.OutputPath);
        if (!extension.Equals(value.Format == RecordingFormat.Mp4 ? ".mp4" : ".gif", StringComparison.OrdinalIgnoreCase))
            throw new ArgumentException("Output extension must match the recording format.");
    }

    public void Dispose()
    {
        bool wait;
        lock (sync)
        {
            if (disposed) return;
            disposed = true;
            if (state == RecordingState.Idle) { DisposeSignal(); return; }
            wait = Environment.CurrentManagedThreadId != workerThread;
        }
        Task<RecordingResult> task = StopAsync();
        if (wait)
        {
            try { task.GetAwaiter().GetResult(); }
            catch (Exception error) { Trace.TraceWarning("Recording disposed after failure: {0}", error.Message); }
            lock (sync) DisposeSignal();
        }
    }

    private void DisposeSignal()
    {
        if (signalDisposed) return;
        signalDisposed = true;
        changed.Dispose();
    }

    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool IsWindow(nint window);
}
