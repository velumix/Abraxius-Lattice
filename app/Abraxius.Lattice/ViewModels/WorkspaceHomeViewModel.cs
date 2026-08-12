using System.Collections.ObjectModel;
using Avalonia.Media;
using Material.Icons;
using Abraxius.Lattice.Services;

namespace Abraxius.Lattice.ViewModels;

public sealed class WorkspaceHomeViewModel : ViewModelBase
{
    private string? _workspacePath;
    private IBrush _studioStatusBrush = Brushes.Gray;
    private string _studioStatus = "Unavailable";
    private string _studioDetail = "No Studio session is bound";
    private string _providerStatus = "None connected";
    private string _providerDetail = "Provider state unavailable";
    private string _indexStatus = "Unavailable";
    private string _indexDetail = "No index is connected";

    public WorkspaceHomeViewModel()
    {
        SelectFolderCommand = new AsyncRelayCommand(SelectFolderAsync);
        var persistedPath = WorkspaceStateStore.Load();
        if (persistedPath is not null && Directory.Exists(persistedPath))
        {
            _workspacePath = persistedPath;
            _indexStatus = "Starting";
            _indexDetail = "Waiting for daemon ingest";
        }
    }

    public string WorkspaceName => _workspacePath is null
        ? "No workspace opened"
        : new DirectoryInfo(_workspacePath).Name is { Length: > 0 } name ? name : _workspacePath;
    public string WorkspaceDescription => _workspacePath ?? "Select a project folder to open it in Lattice.";
    public string? WorkspacePath => _workspacePath;
    public IBrush StudioStatusBrush => _studioStatusBrush;
    public string StudioStatus => _studioStatus;
    public string StudioDetail => _studioDetail;
    public string GitBranch => "Unavailable";
    public string GitDetail => "No repository state";
    public string IndexStatus => _indexStatus;
    public string IndexDetail => _indexDetail;
    public string ProviderStatus => _providerStatus;
    public string ProviderDetail => _providerDetail;
    public string DiagnosticSummary => "—";
    public ObservableCollection<ActivityItemViewModel> RecentActivity { get; } = new();
    public ObservableCollection<DiagnosticSummaryViewModel> Diagnostics { get; } = new();
    public AsyncRelayCommand SelectFolderCommand { get; }

    public Func<Task<string?>>? FolderPicker { get; set; }

    public event Action<string>? WorkspaceSelected;

    private async Task SelectFolderAsync()
    {
        if (FolderPicker is null)
        {
            return;
        }

        string? path;
        try
        {
            path = await FolderPicker().ConfigureAwait(true);
        }
        catch (Exception exception)
        {
            System.Diagnostics.Trace.TraceWarning("Workspace folder picker failed: {0}", exception.Message);
            return;
        }

        if (string.IsNullOrWhiteSpace(path) || !Directory.Exists(path))
        {
            return;
        }

        SetWorkspace(path);
        WorkspaceSelected?.Invoke(path);
    }

    private void SetWorkspace(string path)
    {
        var normalized = Path.GetFullPath(path);
        if (string.Equals(_workspacePath, normalized, StringComparison.Ordinal))
        {
            return;
        }

        _workspacePath = normalized;
        _indexStatus = "Starting";
        _indexDetail = "Waiting for daemon ingest";
        WorkspaceStateStore.Save(normalized);
        OnPropertyChanged(nameof(WorkspaceName));
        OnPropertyChanged(nameof(WorkspaceDescription));
        OnPropertyChanged(nameof(WorkspacePath));
        OnPropertyChanged(nameof(IndexStatus));
        OnPropertyChanged(nameof(IndexDetail));
    }

    public void ApplyWorkspaceReady(DaemonWorkspaceStatus status)
    {
        _indexStatus = "Ready";
        _indexDetail = $"{status.SourceCount} source units · revision {status.Revision}";
        OnPropertyChanged(nameof(IndexStatus));
        OnPropertyChanged(nameof(IndexDetail));
    }

    private static class WorkspaceStateStore
    {
        private static string StatePath => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Abraxius",
            "Lattice",
            "workspace.path");

        public static string? Load()
        {
            try
            {
                var value = File.ReadAllText(StatePath).Trim();
                return string.IsNullOrWhiteSpace(value) ? null : Path.GetFullPath(value);
            }
            catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or ArgumentException)
            {
                System.Diagnostics.Trace.TraceWarning("Workspace state could not be loaded: {0}", exception.Message);
                return null;
            }
        }

        public static void Save(string path)
        {
            try
            {
                var directory = Path.GetDirectoryName(StatePath);
                if (directory is not null)
                {
                    Directory.CreateDirectory(directory);
                }

                File.WriteAllText(StatePath, path + Environment.NewLine);
            }
            catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
            {
                System.Diagnostics.Trace.TraceWarning("Workspace state could not be saved: {0}", exception.Message);
            }
        }
    }

    public void ApplyBridgeStatus(StudioBridgeStatus status)
    {
        _studioStatus = status.StudioConnected
            ? "Connected"
            : status.BridgeAvailable ? "Bridge ready" : "Unavailable";
        _studioDetail = status.StudioConnected
            ? $"{status.SessionCount} Studio session{(status.SessionCount == 1 ? string.Empty : "s")} · bridge RTT {FormatLatency(status.LatencyMs)}"
            : status.Detail;
        _studioStatusBrush = status.StudioConnected
            ? Brushes.LimeGreen
            : status.BridgeAvailable ? Brushes.Goldenrod : Brushes.Gray;
        _providerStatus = status.StudioConnected ? "Studio Bridge" : status.BridgeAvailable ? "Ready" : "None connected";
        _providerDetail = status.Detail;
        OnPropertyChanged(nameof(StudioStatus));
        OnPropertyChanged(nameof(StudioDetail));
        OnPropertyChanged(nameof(StudioStatusBrush));
        OnPropertyChanged(nameof(ProviderStatus));
        OnPropertyChanged(nameof(ProviderDetail));
    }

    private static string FormatLatency(double? latencyMs) => latencyMs is null
        ? "—"
        : latencyMs.Value < 1 ? "<1 ms" : $"{latencyMs.Value:0.0} ms";
}

public sealed class ActivityItemViewModel : ViewModelBase
{
    public string Title { get; init; } = string.Empty;
    public string Detail { get; init; } = string.Empty;
    public string TimeText { get; init; } = string.Empty;
    public IBrush StatusBrush { get; init; } = Brushes.Gray;
    public MaterialIconKind IconKind { get; init; } = IconLibrary.Timeline;
}

public sealed class DiagnosticSummaryViewModel : ViewModelBase
{
    public string Title { get; init; } = string.Empty;
    public string ResourceName { get; init; } = string.Empty;
    public IBrush SeverityBrush { get; init; } = Brushes.Gray;
}
