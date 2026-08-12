using System.Collections.ObjectModel;
using Avalonia.Media;
using Material.Icons;
using Avalonia.Threading;
using Abraxius.Lattice.Services;

namespace Abraxius.Lattice.ViewModels;

public sealed class MainWindowViewModel : ViewModelBase
{
    private NavigationItemViewModel? _selectedNavigation;
    private object _currentWorkspace;
    private object? _currentContextPane;
    private object? _currentInspector;
    private object? _currentBottomPanel;
    private object? _selectedBottomTab;
    private bool _isBottomPanelVisible;
    private double _bottomPanelHeight = 220;
    private readonly StudioBridgeStatusService _bridgeStatus = new();
    private string _studioStatusText = "Bridge unavailable";
    private string _studioStatusShort = "Bridge unavailable";
    private string _studioLatencyText = "—";
    private bool _studioConnected;
    private IBrush _studioStatusBrush = Brushes.Gray;
    private string _providerSummary = "No providers connected";
    private string _bridgeStatusShort = "Bridge unavailable";

    public MainWindowViewModel()
    {
        WorkspaceHome = new WorkspaceHomeViewModel();
        WorkspaceHome.PropertyChanged += OnWorkspaceHomePropertyChanged;
        WorkspaceHome.WorkspaceSelected += OnWorkspaceSelected;
        Explorer = new WorkspaceExplorerViewModel();
        Editor = new EditorViewModel();
        Explorer.FileOpened += OnExplorerFileOpened;
        if (WorkspaceHome.WorkspacePath is { } persistedWorkspace)
        {
            Explorer.SetWorkspace(persistedWorkspace);
            Editor.OpenWorkspace(persistedWorkspace);
        }
        Tools = new ToolsViewModel();
        Connections = new ConnectionsViewModel();
        Graph = new GraphViewModel();
        Timeline = new TimelineViewModel();

        NavigationItems = new ObservableCollection<NavigationItemViewModel>
        {
            new("Workspace", "Workspace", IconLibrary.Workspace),
            new("Editor", "Editor", IconLibrary.Editor),
            new("Search", "Search", IconLibrary.Search),
            new("Graph", "Graph", IconLibrary.Graph),
            new("Timeline", "Timeline", IconLibrary.Timeline),
            new("Tools", "Tools", IconLibrary.Tools),
            new("Connections", "Connections", IconLibrary.Connections),
            new("Diagnostics", "Diagnostics", IconLibrary.Diagnostics),
        };

        BottomTabs = new ObservableCollection<string> { "Activity", "Runtime", "Diagnostics", "Trace" };
        _selectedBottomTab = BottomTabs[0];
        _currentWorkspace = WorkspaceHome;
        _currentContextPane = Explorer;
        _selectedNavigation = NavigationItems[0];

        OpenCommandPaletteCommand = new RelayCommand(() => { });
        OpenSettingsCommand = new RelayCommand(() => { });
        CloseContextPaneCommand = new RelayCommand(() => CurrentContextPane = null);
        CloseInspectorCommand = new RelayCommand(() => CurrentInspector = null);
        CollapseBottomPanelCommand = new RelayCommand(() => IsBottomPanelVisible = false);
        ToggleNavigationCommand = new RelayCommand(() => { });

        _bridgeStatus.StatusChanged += OnBridgeStatusChanged;
        _bridgeStatus.Start();
    }

    public MaterialIconKind SearchIcon => IconLibrary.Search;
    public MaterialIconKind SettingsIcon => IconLibrary.Settings;
    public MaterialIconKind MenuIcon => IconLibrary.Menu;

    public string WorkspaceName => WorkspaceHome.WorkspaceName;
    public string BranchLabel => "—";
    public string StudioStatusText => _studioStatusText;
    public string StudioStatusShort => _studioStatusShort;
    public string StudioLatencyText => _studioLatencyText;
    public bool StudioConnected => _studioConnected;
    public IBrush StudioStatusBrush => _studioStatusBrush;
    public string ProviderSummary => _providerSummary;
    public string GitStatusShort => "Git unavailable";
    public string BridgeStatusShort => _bridgeStatusShort;
    public string IndexStatusShort => WorkspaceHome.IndexStatus == "Ready"
        ? "Index ready"
        : $"Index {WorkspaceHome.IndexStatus.ToLowerInvariant()}";
    public string LatticeVersion => "Lattice 0.1.0";
    public string ContextPaneTitle => "Explorer";
    public string InspectorTitle => "Inspector";

    public ObservableCollection<NavigationItemViewModel> NavigationItems { get; }
    public ObservableCollection<string> BottomTabs { get; }

    public NavigationItemViewModel? SelectedNavigation
    {
        get => _selectedNavigation;
        set
        {
            if (!SetProperty(ref _selectedNavigation, value) || value is null)
            {
                return;
            }

            _currentWorkspace = value.Id switch
            {
                "Editor" => Editor,
                "Tools" => Tools,
                "Connections" => Connections,
                "Graph" => Graph,
                "Timeline" => Timeline,
                _ => WorkspaceHome,
            };
            OnPropertyChanged(nameof(CurrentWorkspace));
        }
    }

    public object CurrentWorkspace
    {
        get => _currentWorkspace;
        private set => SetProperty(ref _currentWorkspace, value);
    }

    public object? CurrentContextPane
    {
        get => _currentContextPane;
        set => SetProperty(ref _currentContextPane, value);
    }

    public object? CurrentInspector
    {
        get => _currentInspector;
        set => SetProperty(ref _currentInspector, value);
    }

    public object? CurrentBottomPanel
    {
        get => _currentBottomPanel;
        set => SetProperty(ref _currentBottomPanel, value);
    }

    public object? SelectedBottomTab
    {
        get => _selectedBottomTab;
        set => SetProperty(ref _selectedBottomTab, value);
    }

    public bool IsBottomPanelVisible
    {
        get => _isBottomPanelVisible;
        set => SetProperty(ref _isBottomPanelVisible, value);
    }

    public double BottomPanelHeight
    {
        get => _bottomPanelHeight;
        set => SetProperty(ref _bottomPanelHeight, value);
    }

    public WorkspaceHomeViewModel WorkspaceHome { get; }
    public WorkspaceExplorerViewModel Explorer { get; }
    public EditorViewModel Editor { get; }
    public ToolsViewModel Tools { get; }
    public ConnectionsViewModel Connections { get; }
    public GraphViewModel Graph { get; }
    public TimelineViewModel Timeline { get; }

    public RelayCommand OpenCommandPaletteCommand { get; }
    public RelayCommand OpenSettingsCommand { get; }
    public RelayCommand CloseContextPaneCommand { get; }
    public RelayCommand CloseInspectorCommand { get; }
    public RelayCommand CollapseBottomPanelCommand { get; }
    public RelayCommand ToggleNavigationCommand { get; }

    private void OnBridgeStatusChanged(StudioBridgeStatus status)
    {
        Dispatcher.UIThread.Post(() =>
        {
            _studioStatusText = status.StudioConnected
                ? "Studio connected"
                : status.BridgeAvailable ? "Bridge ready" : "Bridge unavailable";
            _studioStatusShort = status.StudioConnected ? "Studio live" : "Studio waiting";
            _studioConnected = status.StudioConnected;
            _studioLatencyText = status.LatencyMs is { } latency
                ? $"RTT {FormatLatency(latency)}"
                : "RTT —";
            _studioStatusBrush = status.StudioConnected
                ? Brushes.LimeGreen
                : status.BridgeAvailable ? Brushes.Goldenrod : Brushes.Gray;
            _providerSummary = status.StudioConnected
                ? $"Studio Bridge · {status.SessionCount} session{(status.SessionCount == 1 ? string.Empty : "s")}"
                : status.BridgeAvailable ? "Studio Bridge ready" : "No providers connected";
            _bridgeStatusShort = status.StudioConnected
                ? "Bridge live"
                : status.BridgeAvailable ? "Bridge ready" : "Bridge unavailable";
            WorkspaceHome.ApplyBridgeStatus(status);
            OnPropertyChanged(nameof(StudioStatusText));
            OnPropertyChanged(nameof(StudioStatusShort));
            OnPropertyChanged(nameof(StudioLatencyText));
            OnPropertyChanged(nameof(StudioConnected));
            OnPropertyChanged(nameof(StudioStatusBrush));
            OnPropertyChanged(nameof(ProviderSummary));
            OnPropertyChanged(nameof(BridgeStatusShort));
        });
    }

    private void OnWorkspaceSelected(string path)
    {
        Explorer.SetWorkspace(path);
        Editor.OpenWorkspace(path);
        CurrentContextPane = Explorer;
    }

    private void OnExplorerFileOpened(string path)
    {
        Editor.OpenPath(path);
        var editorNavigation = NavigationItems.FirstOrDefault(item => item.Id == "Editor");
        if (editorNavigation is not null)
        {
            SelectedNavigation = editorNavigation;
        }
    }

    private void OnWorkspaceHomePropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs args)
    {
        if (args.PropertyName is nameof(WorkspaceHomeViewModel.WorkspaceName)
            or nameof(WorkspaceHomeViewModel.WorkspaceDescription)
            or nameof(WorkspaceHomeViewModel.IndexStatus)
            or nameof(WorkspaceHomeViewModel.IndexDetail))
        {
            if (args.PropertyName is nameof(WorkspaceHomeViewModel.WorkspaceName)
                or nameof(WorkspaceHomeViewModel.WorkspaceDescription))
            {
                OnPropertyChanged(nameof(WorkspaceName));
            }

            OnPropertyChanged(nameof(IndexStatusShort));
        }
    }

    public void Dispose()
    {
        _bridgeStatus.StatusChanged -= OnBridgeStatusChanged;
        WorkspaceHome.PropertyChanged -= OnWorkspaceHomePropertyChanged;
        WorkspaceHome.WorkspaceSelected -= OnWorkspaceSelected;
        Explorer.FileOpened -= OnExplorerFileOpened;
        Editor.Dispose();
        _bridgeStatus.Dispose();
    }

    private static string FormatLatency(double latencyMs) => latencyMs < 1
        ? "<1 ms"
        : $"{latencyMs:0.0} ms";
}

public sealed class NavigationItemViewModel(string id, string label, MaterialIconKind iconKind) : ViewModelBase
{
    public string Id { get; } = id;
    public string Label { get; } = label;
    public MaterialIconKind IconKind { get; } = iconKind;
    public bool HasAttention => false;
}

internal static class IconLibrary
{
    public static MaterialIconKind Workspace => MaterialIconKind.Folder;
    public static MaterialIconKind Editor => MaterialIconKind.FileDocumentEditOutline;
    public static MaterialIconKind Search => MaterialIconKind.Magnify;
    public static MaterialIconKind Graph => MaterialIconKind.GraphLine;
    public static MaterialIconKind Timeline => MaterialIconKind.Timeline;
    public static MaterialIconKind Tools => MaterialIconKind.ViewGrid;
    public static MaterialIconKind Connections => MaterialIconKind.LanConnect;
    public static MaterialIconKind Diagnostics => MaterialIconKind.AlertCircleOutline;
    public static MaterialIconKind Settings => MaterialIconKind.Cog;
    public static MaterialIconKind Menu => MaterialIconKind.Menu;
}
