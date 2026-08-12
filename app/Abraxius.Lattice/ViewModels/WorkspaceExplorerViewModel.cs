using System.Collections.ObjectModel;
using Avalonia.Media;
using Material.Icons;

namespace Abraxius.Lattice.ViewModels;

public sealed class WorkspaceExplorerViewModel : ViewModelBase
{
    private const int MaxChildrenPerDirectory = 2_000;
    private string? _rootPath;
    private string _status = "Select a workspace folder to browse its files.";
    private ExplorerNodeViewModel? _selectedNode;

    public ObservableCollection<ExplorerNodeViewModel> Nodes { get; } = new();

    public event Action<string>? FileOpened;

    public ExplorerNodeViewModel? SelectedNode
    {
        get => _selectedNode;
        set
        {
            if (!SetProperty(ref _selectedNode, value) || value is null)
            {
                return;
            }

            Activate(value);
        }
    }

    public string Status
    {
        get => _status;
        private set => SetProperty(ref _status, value);
    }

    public string RootPath => _rootPath ?? string.Empty;

    public void SetWorkspace(string path)
    {
        if (!Directory.Exists(path))
        {
            Nodes.Clear();
            _rootPath = null;
            Status = "Workspace folder is unavailable.";
            OnPropertyChanged(nameof(RootPath));
            return;
        }

        _rootPath = Path.GetFullPath(path);
        Nodes.Clear();
        SelectedNode = null;
        try
        {
            foreach (var entry in ReadDirectory(_rootPath))
            {
                Nodes.Add(entry);
            }

            Status = Nodes.Count == 0
                ? "Workspace folder is empty."
                : $"{Nodes.Count} top-level item{(Nodes.Count == 1 ? string.Empty : "s")} · expand folders to browse";
        }
        catch (Exception exception)
        {
            Status = $"Explorer read failed: {exception.Message}";
        }

        OnPropertyChanged(nameof(RootPath));
    }

    private static IEnumerable<ExplorerNodeViewModel> ReadDirectory(string directoryPath)
    {
        string[] paths;
        try
        {
            paths = Directory.EnumerateFileSystemEntries(directoryPath)
                .OrderBy(path => Directory.Exists(path) ? 0 : 1)
                .ThenBy(path => Path.GetFileName(path), StringComparer.OrdinalIgnoreCase)
                .Take(MaxChildrenPerDirectory)
                .ToArray();
        }
        catch (UnauthorizedAccessException)
        {
            yield break;
        }
        catch (IOException)
        {
            yield break;
        }

        foreach (var path in paths)
        {
            yield return new ExplorerNodeViewModel(Path.GetFileName(path), path, Directory.Exists(path));
        }
    }

    internal static IReadOnlyList<ExplorerNodeViewModel> ReadChildren(string directoryPath)
    {
        return ReadDirectory(directoryPath).ToArray();
    }

    public void Activate(ExplorerNodeViewModel node)
    {
        if (!node.IsDirectory && !node.IsPlaceholder)
        {
            FileOpened?.Invoke(node.Path);
        }
    }
}

public sealed class ExplorerNodeViewModel : ViewModelBase
{
    private bool _childrenLoaded;
    private bool _isExpanded;

    public ExplorerNodeViewModel(string name, string path, bool isDirectory, bool isPlaceholder = false)
    {
        Name = name;
        Path = path;
        IsDirectory = isDirectory;
        IsPlaceholder = isPlaceholder;

        // TreeView needs one child to render an expansion affordance. The
        // placeholder is replaced with real children on Expanded, so a large
        // repository is never recursively scanned during initial layout.
        if (isDirectory && !isPlaceholder)
        {
            Children.Add(new ExplorerNodeViewModel("Loading…", path, false, true));
        }
    }

    public string Name { get; }
    public string Path { get; }
    public bool IsDirectory { get; }
    public bool IsPlaceholder { get; }
    public MaterialIconKind IconKind => IsPlaceholder
        ? MaterialIconKind.DotsHorizontal
        : IsDirectory ? MaterialIconKind.Folder : MaterialIconKind.FileDocumentOutline;
    public ObservableCollection<ExplorerNodeViewModel> Children { get; } = new();

    public bool IsExpanded
    {
        get => _isExpanded;
        set => SetProperty(ref _isExpanded, value);
    }

    public void LoadChildren()
    {
        if (!IsDirectory || IsPlaceholder || _childrenLoaded)
        {
            return;
        }

        _childrenLoaded = true;
        Children.Clear();
        foreach (var child in WorkspaceExplorerViewModel.ReadChildren(Path))
        {
            Children.Add(child);
        }

        OnPropertyChanged(nameof(IconKind));
    }
}
