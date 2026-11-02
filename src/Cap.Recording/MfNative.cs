using System;
using System.Runtime.InteropServices;

namespace Cap.Recording;

internal static unsafe class MfNative
{
    internal static readonly Guid MajorType = new("48eba18e-f8c9-4687-bf11-0a74c9f96a8f");
    internal static readonly Guid Subtype = new("f7e34c9a-42e8-4714-b74b-cb29d72c35e5");
    internal static readonly Guid Video = new("73646976-0000-0010-8000-00aa00389b71");
    internal static readonly Guid H264 = new("34363248-0000-0010-8000-00aa00389b71");
    internal static readonly Guid Nv12 = new("3231564e-0000-0010-8000-00aa00389b71");
    internal static readonly Guid FrameSize = new("1652c33d-d6b2-4012-b834-72030849a37d");
    internal static readonly Guid FrameRate = new("c459a2e8-3d2c-4e44-b132-fee5156c7bb0");
    internal static readonly Guid PixelAspect = new("c6376a1e-8d0a-4027-be45-6d9a0ad39bb6");
    internal static readonly Guid Interlace = new("e2724bb8-e676-4806-b4b2-a8d6efb44ccd");
    internal static readonly Guid Bitrate = new("20332624-fb0d-4d9e-bd0d-cbf6786c102e");

    internal static void Check(int hr)
    {
        if (hr < 0) Marshal.ThrowExceptionForHR(hr);
    }
    internal static void* Slot(nint instance, int slot) => (void*)Marshal.ReadIntPtr(Marshal.ReadIntPtr(instance), slot * IntPtr.Size);
    internal static void Release(ref nint instance)
    {
        if (instance != 0) { Marshal.Release(instance); instance = 0; }
    }
    internal static void UInt32(nint target, Guid key, uint value) =>
        Check(((delegate* unmanaged[Stdcall]<nint, Guid*, uint, int>)Slot(target, 21))(target, &key, value));
    internal static void UInt64(nint target, Guid key, ulong value) =>
        Check(((delegate* unmanaged[Stdcall]<nint, Guid*, ulong, int>)Slot(target, 22))(target, &key, value));
    internal static void GuidValue(nint target, Guid key, Guid value) =>
        Check(((delegate* unmanaged[Stdcall]<nint, Guid*, Guid*, int>)Slot(target, 24))(target, &key, &value));
    internal static void Invoke(nint target, int slot) =>
        Check(((delegate* unmanaged[Stdcall]<nint, int>)Slot(target, slot))(target));

    [DllImport("mfplat.dll", ExactSpelling = true)] internal static extern int MFStartup(int version, int flags);
    [DllImport("mfplat.dll", ExactSpelling = true)] internal static extern int MFShutdown();
    [DllImport("mfplat.dll", ExactSpelling = true)] internal static extern int MFCreateMediaType(out nint type);
    [DllImport("mfplat.dll", ExactSpelling = true)] internal static extern int MFCreateSample(out nint sample);
    [DllImport("mfplat.dll", ExactSpelling = true)] internal static extern int MFCreateMemoryBuffer(int size, out nint buffer);
    [DllImport("mfreadwrite.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    internal static extern int MFCreateSinkWriterFromURL(string path, nint stream, nint attributes, out nint writer);
    [DllImport("ole32.dll", ExactSpelling = true)] internal static extern int CoInitializeEx(nint reserved, int flags);
    [DllImport("ole32.dll", ExactSpelling = true)] internal static extern void CoUninitialize();
}
