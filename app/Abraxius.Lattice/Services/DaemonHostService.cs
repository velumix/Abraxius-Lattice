using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Abraxius.Lattice.Services;

/// <summary>
/// Keeps the native daemon/Studio bridge alive with the desktop shell. The
/// daemon remains the authority; Avalonia only owns its child-process lifetime.
/// </summary>
public sealed class DaemonHostService : IDisposable
{
    private const string DefaultBridgeBind = "127.0.0.1:13471";
    private Process? _process;
    private bool _disposed;

    public event Action<DaemonWorkspaceStatus>? WorkspaceReady;

    public bool IsRunning => _process is { HasExited: false };

    public void Start(string? workspacePath = null)
    {
        if (_disposed || IsRunning)
        {
            return;
        }

        var executable = ResolveDaemonExecutable();
        var bind = Environment.GetEnvironmentVariable("LATTICE_STUDIO_BRIDGE_BIND")
            ?? DefaultBridgeBind;
        var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = executable,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = !string.IsNullOrWhiteSpace(workspacePath),
                RedirectStandardError = !string.IsNullOrWhiteSpace(workspacePath),
            },
            EnableRaisingEvents = true,
        };
        if (string.IsNullOrWhiteSpace(workspacePath))
        {
            process.StartInfo.ArgumentList.Add("--studio-bridge-only");
        }
        else
        {
            process.StartInfo.ArgumentList.Add("--workspace");
            process.StartInfo.ArgumentList.Add(workspacePath);
        }
        process.StartInfo.ArgumentList.Add("--studio-bridge-bind");
        process.StartInfo.ArgumentList.Add(bind);
        process.Exited += OnDaemonExited;
        if (!string.IsNullOrWhiteSpace(workspacePath))
        {
            process.OutputDataReceived += OnDaemonOutput;
            process.ErrorDataReceived += OnDaemonError;
        }

        try
        {
            if (!process.Start())
            {
                process.Dispose();
                Trace.TraceWarning("Lattice daemon did not start");
                return;
            }

            _process = process;
            if (!string.IsNullOrWhiteSpace(workspacePath))
            {
                process.BeginOutputReadLine();
                process.BeginErrorReadLine();
            }
            Trace.TraceInformation(
                "Lattice daemon started (PID {0}, workspace: {1})",
                process.Id,
                workspacePath ?? "none");
        }
        catch (Exception exception) when (exception is InvalidOperationException or System.ComponentModel.Win32Exception)
        {
            process.Dispose();
            Trace.TraceWarning("Lattice daemon could not start: {0}", exception.Message);
        }
    }

    public void OpenWorkspace(string workspacePath)
    {
        if (_disposed || string.IsNullOrWhiteSpace(workspacePath) || !Directory.Exists(workspacePath))
        {
            return;
        }

        StopProcess();
        Start(workspacePath);
    }

    private void OnDaemonExited(object? sender, EventArgs e)
    {
        if (sender is Process process)
        {
            Trace.TraceWarning("Lattice daemon exited with code {0}", process.ExitCode);
        }
    }

    private void OnDaemonOutput(object sender, DataReceivedEventArgs args)
    {
        if (string.IsNullOrWhiteSpace(args.Data))
        {
            return;
        }

        try
        {
            var status = JsonSerializer.Deserialize<DaemonWorkspaceStatus>(args.Data);
            if (status is not null && !string.IsNullOrWhiteSpace(status.WorkspaceId))
            {
                WorkspaceReady?.Invoke(status);
            }
        }
        catch (JsonException)
        {
            // Non-JSON daemon output is diagnostic logging, not workspace state.
        }
    }

    private static void OnDaemonError(object sender, DataReceivedEventArgs args)
    {
        if (!string.IsNullOrWhiteSpace(args.Data))
        {
            Trace.TraceInformation("Lattice daemon: {0}", args.Data);
        }
    }

    private static string ResolveDaemonExecutable()
    {
        var configured = Environment.GetEnvironmentVariable("LATTICE_DAEMON_PATH");
        if (!string.IsNullOrWhiteSpace(configured) && File.Exists(configured))
        {
            return configured;
        }

        var cursor = new DirectoryInfo(AppContext.BaseDirectory);
        while (cursor is not null)
        {
            var debug = Path.Combine(cursor.FullName, "target", "debug", "lattice-daemon");
            if (File.Exists(debug))
            {
                return debug;
            }

            var release = Path.Combine(cursor.FullName, "target", "release", "lattice-daemon");
            if (File.Exists(release))
            {
                return release;
            }

            cursor = cursor.Parent;
        }

        // Installed builds package lattice-daemon beside or on PATH. Let the
        // OS resolver handle that case and report a structured warning if it is
        // absent instead of silently pretending the bridge is available.
        return "lattice-daemon";
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        StopProcess();
    }

    private void StopProcess()
    {
        var process = _process;
        _process = null;
        if (process is null)
        {
            return;
        }

        process.Exited -= OnDaemonExited;
        process.OutputDataReceived -= OnDaemonOutput;
        process.ErrorDataReceived -= OnDaemonError;
        try
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
                process.WaitForExit(2_000);
            }
        }
        catch (InvalidOperationException)
        {
            // The daemon exited between the state check and cleanup.
        }
        catch (System.ComponentModel.Win32Exception)
        {
            // The host can deny process inspection during shutdown; cleanup is
            // best effort and must not prevent the desktop shell from exiting.
        }
        finally
        {
            process.Dispose();
        }
    }
}

public sealed record DaemonWorkspaceStatus(
    [property: JsonPropertyName("workspace_id")]
    string WorkspaceId,
    [property: JsonPropertyName("name")]
    string Name,
    [property: JsonPropertyName("root")]
    string Root,
    [property: JsonPropertyName("revision")]
    long Revision,
    [property: JsonPropertyName("source_count")]
    int SourceCount,
    [property: JsonPropertyName("graph_nodes")]
    int GraphNodes,
    [property: JsonPropertyName("cache_root")]
    string CacheRoot);
