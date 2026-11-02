using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using Cap.Core;

namespace Cap.Recording;

/// <summary>Streaming GIF89a: fixed RGB332 palette, identical-frame coalescing and changed-rectangle encoding.</summary>
internal sealed class GifEncoder : IFrameEncoder
{
    private static readonly byte[] Bayer =
    [
        0,48,12,60,3,51,15,63, 32,16,44,28,35,19,47,31,
        8,56,4,52,11,59,7,55, 40,24,36,20,43,27,39,23,
        2,50,14,62,1,49,13,61, 34,18,46,30,33,17,45,29,
        10,58,6,54,9,57,5,53, 42,26,38,22,41,25,37,21
    ];
    private static readonly byte[] EightLevels = BuildQuantizer(7);
    private static readonly byte[] FourLevels = BuildQuantizer(3);
    private readonly BinaryWriter writer;
    private readonly int width, height;
    private byte[]? pending, previous;
    private long pendingEnd, writtenCentiseconds;
    private bool completed;

    internal GifEncoder(string path, int width, int height)
    {
        if (width is < 1 or > 65535 || height is < 1 or > 65535) throw new ArgumentOutOfRangeException(nameof(width));
        this.width = width;
        this.height = height;
        writer = new BinaryWriter(new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.Read), Encoding.ASCII);
        writer.Write(Encoding.ASCII.GetBytes("GIF89a"));
        writer.Write((ushort)width);
        writer.Write((ushort)height);
        writer.Write((byte)0xf7);
        writer.Write((byte)0);
        writer.Write((byte)0);
        for (int i = 0; i < 256; i++)
        {
            writer.Write((byte)(((i >> 5) & 7) * 255 / 7));
            writer.Write((byte)(((i >> 2) & 7) * 255 / 7));
            writer.Write((byte)((i & 3) * 255 / 3));
        }
        writer.Write(new byte[] { 0x21, 0xff, 11 });
        writer.Write(Encoding.ASCII.GetBytes("NETSCAPE2.0"));
        writer.Write(new byte[] { 3, 1, 0, 0, 0 });
    }

    public void Write(PixelFrame frame, long time, long duration)
    {
        ObjectDisposedException.ThrowIf(completed, this);
        frame.Validate();
        if (frame.Width != width || frame.Height != height || duration <= 0 || time < 0)
            throw new ArgumentException("GIF frame dimensions or time are invalid.");
        byte[] indices = new byte[checked(width * height)];
        for (int y = 0, p = 0, s = 0; y < height; y++)
            for (int x = 0; x < width; x++, p++, s += 4)
            {
                int threshold = Bayer[(y & 7) * 8 + (x & 7)] << 8;
                indices[p] = (byte)((EightLevels[threshold + frame.Pixels[s + 2]] << 5) |
                    (EightLevels[threshold + frame.Pixels[s + 1]] << 2) | FourLevels[threshold + frame.Pixels[s]]);
            }
        if (pending is not null && pending.AsSpan().SequenceEqual(indices))
        {
            pendingEnd = checked(time + duration);
            return;
        }
        FlushPending();
        pending = indices;
        pendingEnd = checked(time + duration);
    }

    private void FlushPending()
    {
        if (pending is null) return;
        // Round the cumulative timeline, avoiding cumulative 24/30/60 FPS rounding drift.
        long target = Math.Max(writtenCentiseconds + 1, (pendingEnd + 50_000) / 100_000);
        long delay = target - writtenCentiseconds;
        int left = 0, top = 0, right = width - 1, bottom = height - 1;
        if (previous is not null)
        {
            left = width; top = height; right = -1; bottom = -1;
            for (int y = 0, p = 0; y < height; y++)
                for (int x = 0; x < width; x++, p++)
                    if (previous[p] != pending[p])
                    {
                        left = Math.Min(left, x); right = Math.Max(right, x);
                        top = Math.Min(top, y); bottom = Math.Max(bottom, y);
                    }
            if (right < 0) { left = top = right = bottom = 0; }
        }
        while (delay > 0)
        {
            ushort part = (ushort)Math.Min(ushort.MaxValue, delay);
            writer.Write(new byte[] { 0x21, 0xf9, 4, 4 }); // Keep previous image (disposal=1).
            writer.Write(part);
            writer.Write((byte)0);
            writer.Write((byte)0);
            writer.Write((byte)0x2c);
            writer.Write((ushort)left); writer.Write((ushort)top);
            writer.Write((ushort)(right - left + 1)); writer.Write((ushort)(bottom - top + 1));
            writer.Write((byte)0);
            WriteLzw(pending, left, top, right, bottom);
            writtenCentiseconds += part;
            delay -= part;
        }
        previous = pending;
        pending = null;
    }

    private static byte[] BuildQuantizer(int maximum)
    {
        var result = new byte[64 * 256];
        for (int threshold = 0; threshold < 64; threshold++)
            for (int value = 0; value < 256; value++)
            {
                int lower = value * maximum / 255;
                if (lower == maximum) { result[threshold * 256 + value] = (byte)lower; continue; }
                int low = lower * 255 / maximum, high = (lower + 1) * 255 / maximum;
                bool upper = (value - low) * 128 > (threshold * 2 + 1) * (high - low);
                result[threshold * 256 + value] = (byte)(lower + (upper ? 1 : 0));
            }
        return result;
    }

    private void WriteLzw(byte[] pixels, int left, int top, int right, int bottom)
    {
        writer.Write((byte)8);
        var output = new GifBits(writer);
        var codes = new Dictionary<int, int>(4096);
        int next = 258, bits = 9;
        output.Write(256, bits);
        int prefix = -1;
        for (int y = top; y <= bottom; y++)
        {
            for (int x = left; x <= right; x++)
            {
                int value = pixels[y * width + x];
                if (prefix < 0) { prefix = value; continue; }
                int key = (prefix << 8) | value;
                if (codes.TryGetValue(key, out int code)) { prefix = code; continue; }
                output.Write(prefix, bits);
                // The decoder's dictionary is one entry behind the encoder's dictionary.
                if (next >= (1 << bits) && bits < 12) bits++;
                if (next < 4096) codes.Add(key, next++);
                else
                {
                    output.Write(256, bits);
                    codes.Clear(); next = 258; bits = 9;
                }
                prefix = value;
            }
        }
        if (prefix >= 0) output.Write(prefix, bits);
        if (next >= (1 << bits) && bits < 12) bits++;
        output.Write(257, bits);
        output.Complete();
    }

    public void Complete()
    {
        if (completed) return;
        FlushPending();
        writer.Write((byte)0x3b);
        writer.Flush();
        completed = true;
    }

    public void Dispose()
    {
        try { Complete(); }
        finally { writer.Dispose(); }
    }

    private sealed class GifBits(BinaryWriter writer)
    {
        private readonly byte[] block = new byte[255];
        private int used, bitCount;
        private uint value;
        internal void Write(int code, int bits)
        {
            value |= (uint)code << bitCount;
            bitCount += bits;
            while (bitCount >= 8)
            {
                Append((byte)value); value >>= 8; bitCount -= 8;
            }
        }
        private void Append(byte item)
        {
            block[used++] = item;
            if (used == 255) Flush();
        }
        private void Flush()
        {
            if (used == 0) return;
            writer.Write((byte)used); writer.Write(block, 0, used); used = 0;
        }
        internal void Complete()
        {
            if (bitCount > 0) Append((byte)value);
            Flush(); writer.Write((byte)0);
        }
    }
}
