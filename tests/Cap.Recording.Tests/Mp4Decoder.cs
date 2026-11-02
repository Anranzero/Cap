using System;
using System.Runtime.InteropServices;
using Cap.Recording;

internal sealed record DecodedVideo(int Width, int Height, int Count, byte[] First, long DurationTicks, Guid Subtype);

internal static unsafe class Mp4Decoder
{
    internal static DecodedVideo Decode(string path)
    {
        MfNative.Check(MfNative.MFStartup(0x20070, 0));
        nint reader = 0, attributes = 0, type = 0, native = 0, current = 0;
        try
        {
            MfNative.Check(MFCreateAttributes(out attributes, 1));
            MfNative.UInt32(attributes, new Guid("fb394f3d-ccf1-42ee-bbb3-f9b845d5681d"), 1);
            MfNative.Check(MFCreateSourceReaderFromURL(path, attributes, out reader));
            nint local;
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, uint, uint, nint*, int>)MfNative.Slot(reader, 5))(reader, 0xfffffffc, 0, &local));
            native = local;
            Guid subtype = GetGuid(native, MfNative.Subtype);
            MfNative.Check(MfNative.MFCreateMediaType(out type));
            MfNative.GuidValue(type, MfNative.MajorType, MfNative.Video);
            MfNative.GuidValue(type, MfNative.Subtype, new Guid("00000016-0000-0010-8000-00aa00389b71"));
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, uint, nint, nint, int>)MfNative.Slot(reader, 7))(reader, 0xfffffffc, 0, type));
            MfNative.Check(((delegate* unmanaged[Stdcall]<nint, uint, nint*, int>)MfNative.Slot(reader, 6))(reader, 0xfffffffc, &local));
            current = local;
            ulong size = GetUInt64(current, MfNative.FrameSize);
            int width = (int)(size >> 32), height = (int)(size & uint.MaxValue), count = 0;
            byte[]? first = null;
            long end = 0;
            while (true)
            {
                uint actual, flags;
                long time;
                nint sample = 0, buffer = 0;
                try
                {
                    MfNative.Check(((delegate* unmanaged[Stdcall]<nint, uint, uint, uint*, uint*, long*, nint*, int>)MfNative.Slot(reader, 9))(reader, 0xfffffffc, 0, &actual, &flags, &time, &sample));
                    if ((flags & 2) != 0) break;
                    if (sample == 0) continue;
                    count++;
                    long duration;
                    MfNative.Check(((delegate* unmanaged[Stdcall]<nint, long*, int>)MfNative.Slot(sample, 37))(sample, &duration));
                    end = Math.Max(end, time + duration);
                    if (first is not null) continue;
                    MfNative.Check(((delegate* unmanaged[Stdcall]<nint, nint*, int>)MfNative.Slot(sample, 41))(sample, &local));
                    buffer = local;
                    nint data;
                    int length;
                    MfNative.Check(((delegate* unmanaged[Stdcall]<nint, nint*, nint, int*, int>)MfNative.Slot(buffer, 3))(buffer, &data, 0, &length));
                    try { first = new byte[length]; Marshal.Copy(data, first, 0, length); }
                    finally { MfNative.Invoke(buffer, 4); }
                }
                finally { MfNative.Release(ref buffer); MfNative.Release(ref sample); }
            }
            return new(width, height, count, first ?? throw new InvalidOperationException("No video frames decoded."), end, subtype);
        }
        finally
        {
            MfNative.Release(ref current); MfNative.Release(ref native); MfNative.Release(ref type);
            MfNative.Release(ref reader); MfNative.Release(ref attributes); MfNative.MFShutdown();
        }
    }
    private static Guid GetGuid(nint type, Guid key)
    {
        Guid value;
        MfNative.Check(((delegate* unmanaged[Stdcall]<nint, Guid*, Guid*, int>)MfNative.Slot(type, 10))(type, &key, &value));
        return value;
    }
    private static ulong GetUInt64(nint type, Guid key)
    {
        ulong value;
        MfNative.Check(((delegate* unmanaged[Stdcall]<nint, Guid*, ulong*, int>)MfNative.Slot(type, 8))(type, &key, &value));
        return value;
    }
    [DllImport("mfplat.dll", ExactSpelling = true)] private static extern int MFCreateAttributes(out nint attributes, int initialSize);
    [DllImport("mfreadwrite.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern int MFCreateSourceReaderFromURL(string path, nint attributes, out nint reader);
}
