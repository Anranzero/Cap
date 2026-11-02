using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Runtime.ExceptionServices;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Cap.Core;

namespace Cap.Recording;

public sealed record RecoveryCandidate(string JournalPath, string OriginalOutputPath, DateTime CreatedUtc,
    int Width, int Height, int FramesPerSecond, RecordingFormat Format);

/// <summary>Re-encodes complete, losslessly compressed journal frames after a failed or interrupted recording.</summary>
public static class RecordingRecovery
{
    public static IReadOnlyList<RecoveryCandidate> FindCandidates(string directory)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(directory);
        var found = new List<RecoveryCandidate>();
        if (!Directory.Exists(directory)) return found;
        foreach (string path in Directory.EnumerateFiles(directory, "*.capjournal", SearchOption.TopDirectoryOnly))
        {
            try
            {
                using var reader = FrameJournal.Open(path, out var header);
                found.Add(new(path, header.OutputPath, header.CreatedUtc, header.Width, header.Height, header.Fps, header.Format));
            }
            catch (Exception error) when (error is IOException or InvalidDataException or JsonException or ArgumentException or UnauthorizedAccessException)
            {
                Trace.TraceWarning("Could not inspect recovery journal {0}: {1}", path, error.Message);
            }
        }
        return found;
    }

    public static Task<RecordingResult> RecoverAsync(string journalPath, string outputPath, CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(journalPath);
        ArgumentException.ThrowIfNullOrWhiteSpace(outputPath);
        return Task.Factory.StartNew(() => Recover(journalPath, outputPath, cancellationToken), cancellationToken,
            TaskCreationOptions.LongRunning, TaskScheduler.Default);
    }

    private static RecordingResult Recover(string journalPath, string outputPath, CancellationToken token)
    {
        outputPath = Path.GetFullPath(outputPath);
        if (File.Exists(outputPath)) throw new IOException("Recovery output already exists. Choose a new file name.");
        using var reader = FrameJournal.Open(journalPath, out var header);
        string temporary = outputPath + "." + Guid.NewGuid().ToString("N") + ".recovering." + (header.Format == RecordingFormat.Mp4 ? "mp4" : "gif");
        try
        {
            long count = 0, end = 0;
            using (var encoder = FrameEncoder.Create(temporary, header.Format, header.Width, header.Height, header.Fps))
            {
                while (FrameJournal.ReadFrame(reader, header, out var frame, out long time, out long duration))
                {
                    token.ThrowIfCancellationRequested();
                    encoder.Write(frame!, time, duration);
                    end = checked(time + duration);
                    count++;
                }
                if (count == 0) throw new InvalidDataException("This journal has no fully written frames to recover.");
                encoder.Complete();
            }
            File.Move(temporary, outputPath, false);
            return new RecordingResult(outputPath, TimeSpan.FromTicks(end), count);
        }
        catch
        {
            try { File.Delete(temporary); }
            catch (Exception cleanupError) when (cleanupError is IOException or UnauthorizedAccessException)
            { Trace.TraceWarning("Could not remove interrupted recovery output {0}: {1}", temporary, cleanupError.Message); }
            throw;
        }
    }
}

internal sealed record JournalHeader(int Version, string OutputPath, DateTime CreatedUtc, int Width, int Height, int Fps, RecordingFormat Format);

internal sealed class FrameJournal : IDisposable
{
    private const uint Commit = 0xca91f00d;
    private readonly FileStream stream;
    private readonly BinaryWriter writer;
    private long lastFlush;
    internal string Path { get; }

    internal FrameJournal(string path, JournalHeader header)
    {
        Path = path;
        stream = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.Read, 64 * 1024, FileOptions.SequentialScan);
        try
        {
            writer = new BinaryWriter(stream, Encoding.UTF8, leaveOpen: true);
            writer.Write(Encoding.ASCII.GetBytes("CAPJ0001"));
            byte[] json = JsonSerializer.SerializeToUtf8Bytes(header);
            writer.Write(json.Length);
            writer.Write(json);
            stream.Flush(true);
            lastFlush = Environment.TickCount64;
        }
        catch (Exception error)
        {
            try { stream.Dispose(); }
            catch (Exception cleanupError)
            { throw new AggregateException("Recovery journal initialization and cleanup both failed.", error, cleanupError); }
            throw;
        }
    }

    internal void Write(PixelFrame frame, long time, long duration)
    {
        using var compressed = new MemoryStream();
        using (var deflater = new DeflateStream(compressed, CompressionLevel.Fastest, leaveOpen: true))
            deflater.Write(frame.Pixels);
        writer.Write(checked((int)compressed.Length));
        writer.Write(time);
        writer.Write(duration);
        compressed.Position = 0;
        compressed.CopyTo(stream);
        writer.Write(Commit);
        if (Environment.TickCount64 - lastFlush >= 1000)
        {
            stream.Flush(true);
            lastFlush = Environment.TickCount64;
            CheckFreeSpace(Path);
        }
    }

    internal static void CheckFreeSpace(string path)
    {
        string? root = System.IO.Path.GetPathRoot(System.IO.Path.GetFullPath(path));
        if (string.IsNullOrEmpty(root)) return;
        var drive = new DriveInfo(root);
        if (drive.IsReady && drive.AvailableFreeSpace < 256L * 1024 * 1024)
            throw new IOException("Recording stopped because less than 256 MB of disk space remains. Completed frames remain in the .capjournal recovery file.");
    }

    internal static BinaryReader Open(string path, out JournalHeader header)
    {
        var reader = new BinaryReader(new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite), Encoding.UTF8);
        try
        {
            if (Encoding.ASCII.GetString(reader.ReadBytes(8)) != "CAPJ0001") throw new InvalidDataException("Invalid Cap recovery journal.");
            int length = reader.ReadInt32();
            if (length is < 2 or > 65536) throw new InvalidDataException("Invalid journal header length.");
            header = JsonSerializer.Deserialize<JournalHeader>(reader.ReadBytes(length)) ?? throw new InvalidDataException("Missing journal header.");
            if (header.Version != 1 || header.Width is < 1 or > 8192 || header.Height is < 1 or > 8192 ||
                (long)header.Width * header.Height > 33_554_432 || header.Fps is < 1 or > 60 || !Enum.IsDefined(header.Format))
                throw new InvalidDataException("Unsupported recovery dimensions or format.");
            return reader;
        }
        catch { reader.Dispose(); throw; }
    }

    internal static bool ReadFrame(BinaryReader reader, JournalHeader header, out PixelFrame? frame, out long time, out long duration)
    {
        frame = null; time = duration = 0;
        if (reader.BaseStream.Position == reader.BaseStream.Length) return false;
        // A power loss may leave only a prefix of the last record. Earlier committed records remain usable.
        if (reader.BaseStream.Length - reader.BaseStream.Position < 20) return false;
        int length = reader.ReadInt32();
        time = reader.ReadInt64(); duration = reader.ReadInt64();
        int rawLength = checked(header.Width * header.Height * 4);
        if (length <= 0 || length > rawLength + rawLength / 8 + 65536 || time < 0 || duration <= 0)
            throw new InvalidDataException("Corrupt recovery frame header.");
        if (reader.BaseStream.Length - reader.BaseStream.Position < length + 4L) return false;
        byte[] compressed = reader.ReadBytes(length);
        if (reader.ReadUInt32() != Commit) throw new InvalidDataException("Recovery frame did not have a valid commit marker.");
        byte[] pixels = new byte[rawLength];
        using var memory = new MemoryStream(compressed, writable: false);
        using var inflater = new DeflateStream(memory, CompressionMode.Decompress);
        inflater.ReadExactly(pixels);
        if (inflater.ReadByte() != -1) throw new InvalidDataException("Recovery frame exceeds expected dimensions.");
        frame = new PixelFrame(header.Width, header.Height, pixels);
        return true;
    }

    public void Dispose()
    {
        List<Exception>? failures = null;
        try { writer.Flush(); stream.Flush(true); }
        catch (Exception error) { (failures ??= new()).Add(error); }
        try { writer.Dispose(); }
        catch (Exception error) { (failures ??= new()).Add(error); }
        try { stream.Dispose(); }
        catch (Exception error) { (failures ??= new()).Add(error); }
        if (failures is { Count: 1 }) ExceptionDispatchInfo.Capture(failures[0]).Throw();
        if (failures is not null) throw new AggregateException("Recovery journal could not be flushed or closed.", failures);
    }
}
