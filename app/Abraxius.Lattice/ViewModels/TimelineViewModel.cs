using System.Collections.ObjectModel;

namespace Abraxius.Lattice.ViewModels;

public sealed class TimelineViewModel : ViewModelBase
{
    public object? Trace => null;
    public object? Viewport => null;
    public ObservableCollection<object> VisibleTracks { get; } = new();
    public object? SelectedRange => null;
    public string TraceSummary => "No trace loaded";

    public RelayCommand ZoomOutCommand { get; } = new(() => { });
    public RelayCommand ZoomInCommand { get; } = new(() => { });
    public RelayCommand FitTraceCommand { get; } = new(() => { });
}
