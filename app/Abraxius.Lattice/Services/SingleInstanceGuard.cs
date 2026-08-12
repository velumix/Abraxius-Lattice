using System.Diagnostics;

namespace Abraxius.Lattice.Services;

/// <summary>
/// Holds an operating-system-backed exclusive lock for the desktop process.
/// The lock file may remain after a crash; the file handle is what represents
/// ownership, so the next launch can safely recover it.
/// </summary>
public sealed class SingleInstanceGuard : IDisposable
{
    private readonly FileStream _lockStream;
    private bool _disposed;

    private SingleInstanceGuard(FileStream lockStream, string lockPath)
    {
        _lockStream = lockStream;
        LockPath = lockPath;
    }

    public string LockPath { get; }

    public static bool TryAcquire(out SingleInstanceGuard? guard)
    {
        guard = null;

        try
        {
            var lockPath = GetLockPath();
            var lockStream = new FileStream(
                lockPath,
                FileMode.OpenOrCreate,
                FileAccess.ReadWrite,
                FileShare.None,
                bufferSize: 1,
                options: FileOptions.WriteThrough);

            // Keep a small diagnostic marker in the file without relying on
            // it for ownership. The exclusive handle is the actual guard.
            lockStream.SetLength(0);
            using (var writer = new StreamWriter(lockStream, leaveOpen: true))
            {
                writer.Write(Environment.ProcessId);
                writer.Flush();
            }
            lockStream.Flush(flushToDisk: true);

            guard = new SingleInstanceGuard(lockStream, lockPath);
            Trace.TraceInformation("Lattice single-instance lock acquired: {0}", lockPath);
            return true;
        }
        catch (IOException)
        {
            Trace.TraceInformation("Lattice is already running; exiting duplicate launch.");
            return false;
        }
        catch (UnauthorizedAccessException exception)
        {
            // Treat an unavailable lock location as a safe startup failure
            // rather than risking two independent desktop instances.
            Trace.TraceError("Lattice could not acquire its single-instance lock: {0}", exception.Message);
            return false;
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _lockStream.Dispose();
        GC.SuppressFinalize(this);
    }

    private static string GetLockPath()
    {
        var appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        if (string.IsNullOrWhiteSpace(appData))
        {
            appData = Path.GetTempPath();
        }

        var directory = Path.Combine(appData, "Abraxius", "Lattice");
        Directory.CreateDirectory(directory);
        return Path.Combine(directory, "lattice.instance.lock");
    }
}
