using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;

namespace Abraxius.Lattice.Controls;

public sealed class GraphSurface : Control
{
    public static readonly StyledProperty<object?> GraphProperty =
        AvaloniaProperty.Register<GraphSurface, object?>(nameof(Graph));

    public static readonly StyledProperty<object?> SelectionProperty =
        AvaloniaProperty.Register<GraphSurface, object?>(nameof(Selection));

    public static readonly StyledProperty<object?> CameraProperty =
        AvaloniaProperty.Register<GraphSurface, object?>(nameof(Camera));

    public static readonly StyledProperty<IBrush?> BackgroundProperty =
        AvaloniaProperty.Register<GraphSurface, IBrush?>(nameof(Background));

    public static readonly StyledProperty<IBrush?> GridBrushProperty =
        AvaloniaProperty.Register<GraphSurface, IBrush?>(nameof(GridBrush));

    public object? Graph
    {
        get => GetValue(GraphProperty);
        set => SetValue(GraphProperty, value);
    }

    public object? Selection
    {
        get => GetValue(SelectionProperty);
        set => SetValue(SelectionProperty, value);
    }

    public object? Camera
    {
        get => GetValue(CameraProperty);
        set => SetValue(CameraProperty, value);
    }

    public IBrush? Background
    {
        get => GetValue(BackgroundProperty);
        set => SetValue(BackgroundProperty, value);
    }

    public IBrush? GridBrush
    {
        get => GetValue(GridBrushProperty);
        set => SetValue(GridBrushProperty, value);
    }

    public override void Render(DrawingContext context)
    {
        base.Render(context);
        context.FillRectangle(Background ?? Brushes.Transparent, new Rect(Bounds.Size));

        if (Graph is null)
        {
            return;
        }

        var pen = new Pen(GridBrush ?? Brushes.Gray, 1);
        var bounds = Bounds;
        var center = bounds.Center;
        context.DrawLine(pen, new Point(center.X, bounds.Top), new Point(center.X, bounds.Bottom));
        context.DrawLine(pen, new Point(bounds.Left, center.Y), new Point(bounds.Right, center.Y));
    }
}
