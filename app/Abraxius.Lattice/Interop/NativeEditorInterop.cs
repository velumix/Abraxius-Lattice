using System.Runtime.InteropServices;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Abraxius.Lattice.Interop;

/// <summary>
/// Managed façade over the small C ABI exposed by lattice-editor-native.
/// ViewModels and controls use this class; raw P/Invoke does not escape it.
/// </summary>
public static class NativeEditorInterop
{
    private const string LibraryName = "lattice_editor_native";
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true,
    };

    static NativeEditorInterop()
    {
        NativeLibrary.SetDllImportResolver(typeof(NativeEditorInterop).Assembly, ResolveLibrary);
    }

    public static NativeEditorSession? TryOpen(
        ulong documentId,
        string source,
        bool readOnly,
        out string? error)
    {
        error = null;
        try
        {
            var session = new NativeEditorSession(documentId, source, readOnly);
            return session;
        }
        catch (Exception exception) when (exception is DllNotFoundException
            or EntryPointNotFoundException
            or InvalidOperationException)
        {
            error = exception.Message;
            return null;
        }
    }

    private static IntPtr ResolveLibrary(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (!string.Equals(libraryName, LibraryName, StringComparison.Ordinal))
        {
            return IntPtr.Zero;
        }

        var fileNames = OperatingSystem.IsWindows()
            ? new[] { "lattice_editor_native.dll" }
            : OperatingSystem.IsMacOS()
                ? new[] { "liblattice_editor_native.dylib" }
                : new[] { "liblattice_editor_native.so" };
        var candidates = fileNames
            .SelectMany(fileName => new[]
            {
                Path.Combine(AppContext.BaseDirectory, fileName),
                Path.Combine(AppContext.BaseDirectory, "native", fileName),
                Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "target", "debug", fileName),
            })
            .Select(Path.GetFullPath)
            .Distinct(StringComparer.Ordinal)
            .ToArray();

        foreach (var candidate in candidates)
        {
            if (File.Exists(candidate))
            {
                return NativeLibrary.Load(candidate);
            }
        }

        return IntPtr.Zero;
    }

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr lattice_editor_create();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void lattice_editor_destroy(IntPtr handle);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_open(
        IntPtr handle,
        ulong documentId,
        IntPtr source,
        nuint length,
        [MarshalAs(UnmanagedType.I1)] bool readOnly);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_close(IntPtr handle, ulong documentId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_insert_text(
        IntPtr handle,
        ulong documentId,
        IntPtr inserted,
        nuint insertedLength);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_delete_backward(IntPtr handle, ulong documentId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_delete_forward(IntPtr handle, ulong documentId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_move_caret(IntPtr handle, ulong documentId, byte movement);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_set_selection(
        IntPtr handle,
        ulong documentId,
        nuint anchor,
        nuint head);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_undo(IntPtr handle, ulong documentId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int lattice_editor_document_redo(IntPtr handle, ulong documentId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr lattice_editor_document_snapshot_json(
        IntPtr handle,
        ulong documentId,
        nuint firstLine,
        nuint lastLine);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr lattice_editor_document_text(IntPtr handle, ulong documentId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr lattice_editor_document_selection_text(IntPtr handle, ulong documentId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr lattice_editor_document_luau_analysis_json(IntPtr handle, ulong documentId);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr lattice_editor_last_error(IntPtr handle);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void lattice_editor_free_string(IntPtr value);

    public sealed class NativeEditorSession : IDisposable
    {
        private readonly object _gate = new();
        private IntPtr _handle;
        private bool _disposed;

        internal NativeEditorSession(ulong documentId, string source, bool readOnly)
        {
            DocumentId = documentId;
            _handle = lattice_editor_create();
            if (_handle == IntPtr.Zero)
            {
                throw new InvalidOperationException("the native editor could not allocate an editor handle");
            }

            try
            {
                InvokeWithUtf8(source, (pointer, length) =>
                {
                    var status = lattice_editor_document_open(_handle, documentId, pointer, length, readOnly);
                    ThrowIfFailed(status);
                });
            }
            catch
            {
                lattice_editor_destroy(_handle);
                _handle = IntPtr.Zero;
                throw;
            }
        }

        public ulong DocumentId { get; }

        public event Action? Changed;

        public EditorViewportSnapshot Snapshot(int firstLine, int lastLine)
        {
            ThrowIfDisposed();
            var value = lattice_editor_document_snapshot_json(
                _handle,
                DocumentId,
                (nuint)Math.Max(0, firstLine),
                (nuint)Math.Max(firstLine, lastLine));
            return Deserialize<EditorViewportSnapshot>(value)
                ?? throw new InvalidOperationException("native editor returned an empty viewport snapshot");
        }

        public string ReadText()
        {
            ThrowIfDisposed();
            return Deserialize<string>(lattice_editor_document_text(_handle, DocumentId))
                ?? throw new InvalidOperationException("native editor returned an empty document");
        }

        public string SelectedText()
        {
            ThrowIfDisposed();
            return Deserialize<string>(lattice_editor_document_selection_text(_handle, DocumentId)) ?? string.Empty;
        }

        public LuauAnalysisSnapshot AnalyzeLuau()
        {
            lock (_gate)
            {
                ThrowIfDisposed();
                return Deserialize<LuauAnalysisSnapshot>(
                    lattice_editor_document_luau_analysis_json(_handle, DocumentId))
                    ?? throw new InvalidOperationException("official Luau analysis returned no result");
            }
        }

        public void InsertText(string text)
        {
            Mutate(() => InvokeWithUtf8(text, (pointer, length) =>
            {
                ThrowIfFailed(lattice_editor_document_insert_text(_handle, DocumentId, pointer, length));
            }));
        }

        public void DeleteBackward() => Mutate(() => ThrowIfFailed(lattice_editor_document_delete_backward(_handle, DocumentId)));

        public void DeleteForward() => Mutate(() => ThrowIfFailed(lattice_editor_document_delete_forward(_handle, DocumentId)));

        public void MoveCaret(byte movement) => Mutate(() => ThrowIfFailed(lattice_editor_document_move_caret(_handle, DocumentId, movement)), notify: true);

        public void SetSelection(int anchor, int head)
        {
            Mutate(() => ThrowIfFailed(lattice_editor_document_set_selection(
                _handle,
                DocumentId,
                (nuint)Math.Max(0, anchor),
                (nuint)Math.Max(0, head))), notify: true);
        }

        public void Undo() => Mutate(() => ThrowIfFailed(lattice_editor_document_undo(_handle, DocumentId)));

        public void Redo() => Mutate(() => ThrowIfFailed(lattice_editor_document_redo(_handle, DocumentId)));

        public void Dispose()
        {
            lock (_gate)
            {
                if (_disposed)
                {
                    return;
                }

                _disposed = true;
                if (_handle != IntPtr.Zero)
                {
                    _ = lattice_editor_document_close(_handle, DocumentId);
                    lattice_editor_destroy(_handle);
                    _handle = IntPtr.Zero;
                }
            }
        }

        private void Mutate(Action operation, bool notify = true)
        {
            ThrowIfDisposed();
            operation();
            if (notify)
            {
                Changed?.Invoke();
            }
        }

        private void ThrowIfDisposed()
        {
            if (_disposed || _handle == IntPtr.Zero)
            {
                throw new ObjectDisposedException(nameof(NativeEditorSession));
            }
        }

        private void ThrowIfFailed(int status)
        {
            if (status == 0)
            {
                return;
            }

            var error = DeserializeCString(lattice_editor_last_error(_handle));
            throw new InvalidOperationException(error ?? "native editor operation failed");
        }

        private static void InvokeWithUtf8(string value, Action<IntPtr, nuint> operation)
        {
            var bytes = Encoding.UTF8.GetBytes(value);
            var pointer = bytes.Length == 0 ? IntPtr.Zero : Marshal.AllocHGlobal(bytes.Length);
            try
            {
                if (pointer != IntPtr.Zero)
                {
                    Marshal.Copy(bytes, 0, pointer, bytes.Length);
                }
                operation(pointer, (nuint)bytes.Length);
            }
            finally
            {
                if (pointer != IntPtr.Zero)
                {
                    Marshal.FreeHGlobal(pointer);
                }
            }
        }

        private static T? Deserialize<T>(IntPtr pointer)
        {
            var json = DeserializeCString(pointer);
            return json is null ? default : JsonSerializer.Deserialize<T>(json, JsonOptions);
        }

        private static string? DeserializeCString(IntPtr pointer)
        {
            if (pointer == IntPtr.Zero)
            {
                return null;
            }

            try
            {
                return Marshal.PtrToStringUTF8(pointer);
            }
            finally
            {
                lattice_editor_free_string(pointer);
            }
        }
    }
}

public sealed class EditorViewportSnapshot
{
    [JsonPropertyName("document_id")] public ulong DocumentId { get; init; }
    [JsonPropertyName("revision")] public ulong Revision { get; init; }
    [JsonPropertyName("content_hash")] public string? ContentHash { get; init; }
    [JsonPropertyName("first_line")] public int FirstLine { get; init; }
    [JsonPropertyName("last_line")] public int LastLine { get; init; }
    [JsonPropertyName("total_lines")] public int TotalLines { get; init; }
    [JsonPropertyName("total_bytes")] public int TotalBytes { get; init; }
    [JsonPropertyName("selection")] public EditorSelectionSnapshot Selection { get; init; } = new();
    [JsonPropertyName("modified")] public bool Modified { get; init; }
    [JsonPropertyName("read_only")] public bool ReadOnly { get; init; }
    [JsonPropertyName("lines")] public IReadOnlyList<EditorViewportLine> Lines { get; init; } = Array.Empty<EditorViewportLine>();
}

public sealed class EditorViewportLine
{
    [JsonPropertyName("line_index")] public int LineIndex { get; init; }
    [JsonPropertyName("number")] public int Number { get; init; }
    [JsonPropertyName("start_byte")] public int StartByte { get; init; }
    [JsonPropertyName("text")] public string Text { get; init; } = string.Empty;
}

public sealed class EditorSelectionSnapshot
{
    [JsonPropertyName("anchor")] public int Anchor { get; init; }
    [JsonPropertyName("head")] public int Head { get; init; }
}

public sealed class LuauAnalysisSnapshot
{
    [JsonPropertyName("symbols")] public IReadOnlyList<LuauSymbolSnapshot> Symbols { get; init; } = Array.Empty<LuauSymbolSnapshot>();
    [JsonPropertyName("references")] public IReadOnlyList<LuauReferenceSnapshot> References { get; init; } = Array.Empty<LuauReferenceSnapshot>();
    [JsonPropertyName("requires")] public IReadOnlyList<LuauRequireSnapshot> Requires { get; init; } = Array.Empty<LuauRequireSnapshot>();
    [JsonPropertyName("diagnostics")] public IReadOnlyList<LuauDiagnosticSnapshot> Diagnostics { get; init; } = Array.Empty<LuauDiagnosticSnapshot>();
    [JsonPropertyName("line_count")] public long LineCount { get; init; }
}

public sealed class LuauSymbolSnapshot
{
    [JsonPropertyName("name")] public string Name { get; init; } = string.Empty;
    [JsonPropertyName("kind")] public string Kind { get; init; } = string.Empty;
    [JsonPropertyName("span")] public LuauSpanSnapshot Span { get; init; } = new();
}

public sealed class LuauReferenceSnapshot
{
    [JsonPropertyName("name")] public string Name { get; init; } = string.Empty;
    [JsonPropertyName("kind")] public string Kind { get; init; } = string.Empty;
    [JsonPropertyName("span")] public LuauSpanSnapshot Span { get; init; } = new();
}

public sealed class LuauRequireSnapshot
{
    [JsonPropertyName("specifier")] public string Specifier { get; init; } = string.Empty;
    [JsonPropertyName("span")] public LuauSpanSnapshot Span { get; init; } = new();
}

public sealed class LuauDiagnosticSnapshot
{
    [JsonPropertyName("message")] public string Message { get; init; } = string.Empty;
    [JsonPropertyName("span")] public LuauSpanSnapshot Span { get; init; } = new();
}

public sealed class LuauSpanSnapshot
{
    [JsonPropertyName("begin")] public LuauPositionSnapshot Begin { get; init; } = new();
    [JsonPropertyName("end")] public LuauPositionSnapshot End { get; init; } = new();
}

public sealed class LuauPositionSnapshot
{
    [JsonPropertyName("line")] public uint Line { get; init; }
    [JsonPropertyName("column")] public uint Column { get; init; }
}
