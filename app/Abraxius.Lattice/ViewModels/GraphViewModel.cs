namespace Abraxius.Lattice.ViewModels;

public sealed class GraphViewModel : ViewModelBase
{
    public object? Graph => null;
    public object? Selection => null;
    public object? Camera => null;
    public string VisibleGraphSummary => "No graph loaded";

    public RelayCommand BackCommand { get; } = new(() => { });
    public RelayCommand ForwardCommand { get; } = new(() => { });
    public RelayCommand FocusCommand { get; } = new(() => { });
    public RelayCommand FitGraphCommand { get; } = new(() => { });
    public RelayCommand ResetZoomCommand { get; } = new(() => { });
}
