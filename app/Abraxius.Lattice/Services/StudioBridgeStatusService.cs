using System.Diagnostics;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Abraxius.Lattice.Services;

public sealed record StudioBridgeStatus(
    bool BridgeAvailable,
    bool StudioConnected,
    int SessionCount,
    double? LatencyMs,
    string Detail);

/// <summary>
/// Reads the daemon-owned Studio bridge state for presentation. It never
/// infers Studio health from process names or fabricates a connected state.
/// </summary>
public sealed class StudioBridgeStatusService : IDisposable
{
    private const long SessionTtlMs = 60_000;
    private static readonly Uri BridgeBaseUri = new("http://127.0.0.1:13471");
    private readonly HttpClient _http = new() { BaseAddress = BridgeBaseUri, Timeout = TimeSpan.FromSeconds(2) };
    private readonly CancellationTokenSource _shutdown = new();
    private Task? _pollTask;
    private string? _sessionToken;
    private long _sessionExpiresUnixMs;
    private bool _disposed;

    public event Action<StudioBridgeStatus>? StatusChanged;

    public void Start()
    {
        if (_disposed || _pollTask is not null)
        {
            return;
        }

        _pollTask = Task.Run(PollLoopAsync);
    }

    private async Task PollLoopAsync()
    {
        while (!_shutdown.IsCancellationRequested)
        {
            StudioBridgeStatus status;
            try
            {
                status = await PollOnceAsync(_shutdown.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (_shutdown.IsCancellationRequested)
            {
                break;
            }
            catch (Exception exception) when (exception is HttpRequestException or JsonException or TaskCanceledException)
            {
                status = new StudioBridgeStatus(false, false, 0, null, "Bridge unavailable");
                Trace.TraceWarning("Studio bridge status unavailable: {0}", exception.Message);
            }

            StatusChanged?.Invoke(status);
            try
            {
                await Task.Delay(TimeSpan.FromSeconds(1), _shutdown.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (_shutdown.IsCancellationRequested)
            {
                break;
            }
        }
    }

    private async Task<StudioBridgeStatus> PollOnceAsync(CancellationToken cancellationToken)
    {
        if (string.IsNullOrEmpty(_sessionToken) || DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() >= _sessionExpiresUnixMs - 5_000)
        {
            await PairAsync(cancellationToken).ConfigureAwait(false);
        }

        using var request = new HttpRequestMessage(HttpMethod.Get, "/v1/studio-bridge/sessions");
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _sessionToken);
        var stopwatch = Stopwatch.StartNew();
        using var response = await _http.SendAsync(request, cancellationToken).ConfigureAwait(false);
        stopwatch.Stop();
        if (response.StatusCode == System.Net.HttpStatusCode.Unauthorized)
        {
            _sessionToken = null;
            throw new HttpRequestException("Studio bridge session expired");
        }

        response.EnsureSuccessStatusCode();
        var sessions = await response.Content.ReadFromJsonAsync<List<BridgeSession>>(
            cancellationToken: cancellationToken).ConfigureAwait(false) ?? [];
        var nowUnixMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var activeSessions = sessions.Count(session =>
            !string.Equals(session.State, "stale", StringComparison.OrdinalIgnoreCase)
            && (session.LastSeenUnixMs is null || nowUnixMs - session.LastSeenUnixMs <= SessionTtlMs));
        var detail = activeSessions == 0
            ? "Bridge ready · waiting for Studio companion"
            : $"{activeSessions} Studio session{(activeSessions == 1 ? string.Empty : "s")}";
        // Keep the fractional value.  A loopback request commonly completes in
        // less than one millisecond; using ElapsedMilliseconds truncates that
        // to 0 and makes the UI look as though no measurement occurred.
        return new StudioBridgeStatus(true, activeSessions > 0, activeSessions, stopwatch.Elapsed.TotalMilliseconds, detail);
    }

    private async Task PairAsync(CancellationToken cancellationToken)
    {
        var discovery = await _http.GetFromJsonAsync<DiscoveryResponse>(
            "/v1/studio-bridge/discover", cancellationToken).ConfigureAwait(false)
            ?? throw new HttpRequestException("Studio bridge discovery returned no response");
        if (string.IsNullOrWhiteSpace(discovery.Challenge))
        {
            throw new JsonException("Studio bridge discovery returned no challenge");
        }

        using var response = await _http.PostAsJsonAsync(
            "/v1/studio-bridge/pair",
            new PairRequest(discovery.Challenge, "lattice-desktop", "Abraxius Lattice"),
            cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var pairing = await response.Content.ReadFromJsonAsync<PairResponse>(
            cancellationToken: cancellationToken).ConfigureAwait(false)
            ?? throw new JsonException("Studio bridge pairing returned no response");
        if (string.IsNullOrWhiteSpace(pairing.SessionToken))
        {
            throw new JsonException("Studio bridge pairing returned no session token");
        }

        _sessionToken = pairing.SessionToken;
        _sessionExpiresUnixMs = pairing.SessionExpiresUnixMs;
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _shutdown.Cancel();
        _http.Dispose();
        _shutdown.Dispose();
    }

    private sealed record DiscoveryResponse(
        [property: JsonPropertyName("challenge")] string Challenge);

    private sealed record PairRequest(
        [property: JsonPropertyName("challenge")] string Challenge,
        [property: JsonPropertyName("client_kind")] string ClientKind,
        [property: JsonPropertyName("client_name")] string ClientName);

    private sealed record PairResponse(
        [property: JsonPropertyName("session_token")] string SessionToken,
        [property: JsonPropertyName("session_expires_unix_ms")] long SessionExpiresUnixMs);

    private sealed record BridgeSession(
        [property: JsonPropertyName("state")] string? State,
        [property: JsonPropertyName("last_seen_unix_ms")] long? LastSeenUnixMs);
}
