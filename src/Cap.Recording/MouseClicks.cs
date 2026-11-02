using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Threading;

namespace Cap.Recording;

internal sealed record MouseClick(int X, int Y, long Time);

/// <summary>A message-pumped low-level hook catches presses even when they occur between video frames.</summary>
internal sealed class MouseClicks : IDisposable
{
    private readonly Thread thread;
    private readonly Hook callback;
    private readonly ManualResetEventSlim ready = new();
    private MouseClick latest = new(0, 0, 0);
    private uint threadId;
    private Exception? error;
    private nint hook;
    private bool disposed;
    internal MouseClick Snapshot => Volatile.Read(ref latest);

    internal MouseClicks()
    {
        callback = OnMouse;
        thread = new Thread(Run) { IsBackground = true, Name = "Cap mouse click overlay" };
        thread.Start();
        ready.Wait();
        if (error is not null) { ready.Dispose(); throw new InvalidOperationException("Mouse click highlighting could not start.", error); }
    }

    private void Run()
    {
        threadId = GetCurrentThreadId();
        PeekMessage(out _, 0, 0, 0, 0);
        hook = SetWindowsHookEx(14, callback, 0, 0);
        if (hook == 0) { error = new Win32Exception(Marshal.GetLastWin32Error()); ready.Set(); return; }
        ready.Set();
        try
        {
            int result;
            while ((result = GetMessage(out var message, 0, 0, 0)) > 0)
            {
                TranslateMessage(ref message);
                DispatchMessage(ref message);
            }
            if (result < 0) System.Diagnostics.Trace.TraceError("Cap mouse hook message pump failed: {0}", Marshal.GetLastWin32Error());
        }
        finally { UnhookWindowsHookEx(hook); hook = 0; }
    }

    private nint OnMouse(int code, nint message, nint data)
    {
        if (code >= 0 && message is 0x201 or 0x204 or 0x207 or 0x20b)
        {
            var point = Marshal.PtrToStructure<MouseData>(data);
            Volatile.Write(ref latest, new MouseClick(point.X, point.Y, Environment.TickCount64));
        }
        return CallNextHookEx(hook, code, message, data);
    }

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        if (thread.IsAlive)
        {
            if (!PostThreadMessage(threadId, 0x12, 0, 0))
                System.Diagnostics.Trace.TraceError("Could not stop Cap mouse message pump: {0}", Marshal.GetLastWin32Error());
            thread.Join(TimeSpan.FromSeconds(2));
        }
        ready.Dispose();
        GC.KeepAlive(callback);
    }

    private delegate nint Hook(int code, nint message, nint data);
    [StructLayout(LayoutKind.Sequential)] private struct MouseData { internal int X, Y; internal uint Data, Flags, Time; internal nuint Extra; }
    [StructLayout(LayoutKind.Sequential)] private struct Message { internal nint Window; internal uint Id; internal nuint WParam; internal nint LParam; internal uint Time; internal int X, Y; }
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
    [DllImport("user32.dll", SetLastError = true)] private static extern nint SetWindowsHookEx(int kind, Hook callback, nint module, uint thread);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool UnhookWindowsHookEx(nint hook);
    [DllImport("user32.dll")] private static extern nint CallNextHookEx(nint hook, int code, nint message, nint data);
    [DllImport("user32.dll", SetLastError = true)] private static extern int GetMessage(out Message message, nint window, uint min, uint max);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool PeekMessage(out Message message, nint window, uint min, uint max, uint remove);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool TranslateMessage(ref Message message);
    [DllImport("user32.dll")] private static extern nint DispatchMessage(ref Message message);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool PostThreadMessage(uint thread, uint message, nuint wparam, nint lparam);
}
