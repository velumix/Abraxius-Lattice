using System.Text;
using Avalonia.Threading;
using Abraxius.Lattice.Interop;

namespace Abraxius.Lattice.ViewModels;

public sealed class EditorViewModel : ViewModelBase, IDisposable
{
    private static long _nextDocumentId = 1;
    private NativeEditorInterop.NativeEditorSession? _nativeSession;
    private EditorViewportSnapshot? _lastSnapshot;
    private string _documentName = "No document";
    private string _documentPath = string.Empty;
    private string _statusText = "Select a Luau source file from Explorer.";
    private string? _nativeError;
    private LuauAnalysisSnapshot? _luauAnalysis;
    private CancellationTokenSource? _analysisCancellation;
    private ulong _analysisRevision = ulong.MaxValue;

    public EditorViewModel()
    {
        OpenCommand = new RelayCommand(() => { });
    }

    public NativeEditorInterop.NativeEditorSession? NativeSession => _nativeSession;
    public string DocumentName => _documentName;
    public string DocumentPath => _documentPath;
    public string StatusText => _statusText;
    public string NativeStatus => _nativeError is null ? "Rust editor core" : "Native editor unavailable";
    public bool HasDocument => _nativeSession is not null;
    public bool IsModified => _lastSnapshot?.Modified == true;
    public bool IsLuauDocument => _documentPath.EndsWith(".luau", StringComparison.OrdinalIgnoreCase)
        || _documentPath.EndsWith(".lua", StringComparison.OrdinalIgnoreCase)
        || _documentName.EndsWith(".luau", StringComparison.OrdinalIgnoreCase)
        || _documentName.EndsWith(".lua", StringComparison.OrdinalIgnoreCase);
    public LuauAnalysisSnapshot? LuauAnalysis => _luauAnalysis;
    public string DiagnosticsText => !IsLuauDocument
        ? "Luau analysis: not applicable"
        : _luauAnalysis is null
        ? "Luau analysis pending"
        : _luauAnalysis.Diagnostics.Count == 0
            ? "Luau diagnostics: none"
            : $"Luau diagnostics: {_luauAnalysis.Diagnostics.Count}";
    public RelayCommand OpenCommand { get; }

    public void OpenWorkspace(string workspacePath)
    {
        if (!Directory.Exists(workspacePath))
        {
            return;
        }

        var candidate = EnumerateSourceFiles(workspacePath).FirstOrDefault();
        if (candidate is not null)
        {
            OpenPath(candidate);
        }
        else
        {
            OpenBuffer("untitled.luau", "-- Select a Luau source file from the workspace Explorer.\n");
        }
    }

    public void OpenPath(string path)
    {
        try
        {
            var source = File.ReadAllText(path, Encoding.UTF8);
            OpenBuffer(Path.GetFileName(path), source, Path.GetFullPath(path));
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or DecoderFallbackException)
        {
            _statusText = $"Could not open {Path.GetFileName(path)}: {exception.Message}";
            OnPropertyChanged(nameof(StatusText));
        }
    }

    public void OpenBuffer(string name, string source, string? path = null)
    {
        if (_nativeSession is not null)
        {
            _nativeSession.Changed -= OnNativeSessionChanged;
        }
        CancelLuauAnalysis();
        _nativeSession?.Dispose();
        _nativeSession = null;
        _lastSnapshot = null;
        _documentName = name;
        _documentPath = path ?? string.Empty;
        _nativeError = null;

        var documentId = unchecked((ulong)Interlocked.Increment(ref _nextDocumentId));
        _nativeSession = NativeEditorInterop.TryOpen(documentId, source, readOnly: false, out _nativeError);
        if (_nativeSession is not null)
        {
            _nativeSession.Changed += OnNativeSessionChanged;
        }
        _luauAnalysis = null;
        _analysisRevision = ulong.MaxValue;
        _statusText = _nativeSession is null
            ? _nativeError ?? "Native editor core is unavailable. Build lattice-editor-native and place it beside the app."
            : $"{name} · native Rust buffer";
        OnPropertyChanged(nameof(NativeSession));
        OnPropertyChanged(nameof(DocumentName));
        OnPropertyChanged(nameof(DocumentPath));
        OnPropertyChanged(nameof(IsLuauDocument));
        OnPropertyChanged(nameof(StatusText));
        OnPropertyChanged(nameof(NativeStatus));
        OnPropertyChanged(nameof(HasDocument));
        OnPropertyChanged(nameof(IsModified));
        OnPropertyChanged(nameof(LuauAnalysis));
        OnPropertyChanged(nameof(DiagnosticsText));
    }

    public EditorViewportSnapshot? Snapshot(int firstLine, int lastLine)
    {
        if (_nativeSession is null)
        {
            return null;
        }

        _lastSnapshot = _nativeSession.Snapshot(firstLine, lastLine);
        // Snapshot is called by EditorSurface.Render(). Do not raise property
        // notifications from this method: Avalonia rejects visual invalidation
        // while a render pass is active. Mutation and analysis completion paths
        // publish their notifications outside rendering instead.
        if (IsLuauDocument)
        {
            ScheduleLuauAnalysis(_lastSnapshot.Revision);
        }
        return _lastSnapshot;
    }

    public void SetSelection(int anchor, int head)
    {
        _nativeSession?.SetSelection(anchor, head);
    }

    public void Dispose()
    {
        CancelLuauAnalysis();
        if (_nativeSession is not null)
        {
            _nativeSession.Changed -= OnNativeSessionChanged;
        }
        _nativeSession?.Dispose();
        _nativeSession = null;
    }

    private void OnNativeSessionChanged()
    {
        OnPropertyChanged(nameof(IsModified));
        // The next viewport request supplies the exact new revision. This
        // avoids copying the document or asking the analyzer for stale text.
        Dispatcher.UIThread.Post(InvalidateEditorState);
    }

    private void InvalidateEditorState()
    {
        OnPropertyChanged(nameof(NativeSession));
        OnPropertyChanged(nameof(IsModified));
    }

    private void ScheduleLuauAnalysis(ulong revision)
    {
        if (_nativeSession is null || revision == _analysisRevision)
        {
            return;
        }

        _analysisRevision = revision;
        CancelLuauAnalysis();
        var cancellation = new CancellationTokenSource();
        _analysisCancellation = cancellation;
        var session = _nativeSession;
        _ = AnalyzeLuauAsync(session, revision, cancellation.Token);
    }

    private void CancelLuauAnalysis()
    {
        var cancellation = _analysisCancellation;
        _analysisCancellation = null;
        if (cancellation is null)
        {
            return;
        }

        cancellation.Cancel();
        cancellation.Dispose();
    }

    private async Task AnalyzeLuauAsync(
        NativeEditorInterop.NativeEditorSession session,
        ulong revision,
        CancellationToken cancellationToken)
    {
        try
        {
            var analysis = await Task.Run(session.AnalyzeLuau, cancellationToken).ConfigureAwait(false);
            if (cancellationToken.IsCancellationRequested || !ReferenceEquals(session, _nativeSession))
            {
                return;
            }

            Dispatcher.UIThread.Post(() =>
            {
                if (ReferenceEquals(session, _nativeSession) && revision == _analysisRevision)
                {
                    _luauAnalysis = analysis;
                    OnPropertyChanged(nameof(LuauAnalysis));
                    OnPropertyChanged(nameof(DiagnosticsText));
                }
            });
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception exception)
        {
            System.Diagnostics.Trace.TraceWarning("Luau analysis failed: {0}", exception.Message);
        }
    }

    private static IEnumerable<string> EnumerateSourceFiles(string root)
    {
        var pending = new Queue<(string Path, int Depth)>();
        pending.Enqueue((root, 0));
        while (pending.Count > 0)
        {
            var (directory, depth) = pending.Dequeue();
            IEnumerable<string> entries;
            try
            {
                entries = Directory.EnumerateFileSystemEntries(directory);
            }
            catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
            {
                continue;
            }

            foreach (var entry in entries.OrderBy(value => value, StringComparer.OrdinalIgnoreCase))
            {
                if (Directory.Exists(entry))
                {
                    var name = Path.GetFileName(entry);
                    if (depth < 8 && name is not ".git" and not "target" and not "node_modules")
                    {
                        pending.Enqueue((entry, depth + 1));
                    }
                    continue;
                }

                if (entry.EndsWith(".luau", StringComparison.OrdinalIgnoreCase)
                    || entry.EndsWith(".lua", StringComparison.OrdinalIgnoreCase))
                {
                    yield return entry;
                }
            }
        }
    }
}
