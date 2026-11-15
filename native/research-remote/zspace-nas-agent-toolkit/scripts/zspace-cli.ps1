$ErrorActionPreference = "Stop"
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = $Utf8NoBom
[Console]::OutputEncoding = $Utf8NoBom
$OutputEncoding = $Utf8NoBom

# Only a dedicated powershell -File launcher owns its host process. An invoked
# script (& launcher.ps1) must return to the existing host instead.
$StandaloneLauncher = $false
$HostArguments = [Environment]::GetCommandLineArgs()
for ($Index = 0; $Index -lt ($HostArguments.Length - 1); $Index++) {
    if ($HostArguments[$Index] -ieq '-File') {
        try { $StandaloneLauncher = [IO.Path]::GetFullPath($HostArguments[$Index + 1]) -ieq $PSCommandPath } catch {}
        break
    }
}
function Complete-Launcher([int]$Code) {
    foreach ($Stream in @($Output, $DownloadInput, $Client, $Handler)) {
        if ($Stream -is [IDisposable]) { try { $Stream.Dispose() } catch {} }
    }
    if ($Temp -and (Test-Path -LiteralPath $Temp)) {
        Remove-Item -LiteralPath $Temp -Force -ErrorAction SilentlyContinue
    }
    if ($StandaloneLauncher) {
        [Console]::Out.Flush()
        [Console]::Error.Flush()
        if (-not ('ZspaceLauncher.NativeExit' -as [type])) {
            Add-Type -TypeDefinition 'namespace ZspaceLauncher { public static class NativeExit { [System.Runtime.InteropServices.DllImport("kernel32.dll")] public static extern void ExitProcess(uint code); } }'
        }
        [ZspaceLauncher.NativeExit]::ExitProcess([uint32]$Code)
    }
    $global:LASTEXITCODE = $Code
}

function Fail([string]$Code, [string]$Message) {
    [Console]::Error.WriteLine((@{ok=$false; error=@{code=$Code; message=$Message}} | ConvertTo-Json -Compress))
    Complete-Launcher 2
    exit 2
}

function ConvertTo-NativeArgument([string]$Value) {
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') { return $Value }
    $Builder = New-Object Text.StringBuilder
    [void]$Builder.Append('"')
    $Backslashes = 0
    foreach ($Character in $Value.ToCharArray()) {
        if ($Character -eq '\') { $Backslashes++; continue }
        if ($Character -eq '"') {
            [void]$Builder.Append(('\' * ($Backslashes * 2 + 1)))
            [void]$Builder.Append('"')
        } else {
            [void]$Builder.Append(('\' * $Backslashes))
            [void]$Builder.Append($Character)
        }
        $Backslashes = 0
    }
    [void]$Builder.Append(('\' * ($Backslashes * 2)))
    [void]$Builder.Append('"')
    return $Builder.ToString()
}

function Invoke-Cli([string]$Path, [object[]]$Arguments) {
    $ForwardCode = @($Arguments) -contains '--auth-code-stdin'
    $StartInfo = New-Object Diagnostics.ProcessStartInfo
    $StartInfo.FileName = $Path
    $StartInfo.UseShellExecute = $false
    $StartInfo.RedirectStandardInput = $ForwardCode
    $StartInfo.Arguments = (($Arguments | ForEach-Object { ConvertTo-NativeArgument ([string]$_) }) -join ' ')
    try { $Process = [Diagnostics.Process]::Start($StartInfo) }
    catch { Fail "E_RUNTIME_CACHE" ("Unable to start zspace-cli: " + $_.Exception.Message) }
    # Explicitly forward the one-line authorization input. Ordinary commands
    # do not read the launcher's stdin.
    try {
        if ($ForwardCode) {
            # The child emits its authorization URL before waiting for this line.
            # Use synchronous Console input on .NET Framework; never log the line.
            try {
                if (-not ('ZspaceLauncher.InputReader' -as [type])) {
                    Add-Type -TypeDefinition 'namespace ZspaceLauncher { public static class InputReader { public static System.Threading.Tasks.Task<string> ReadLine() { var source = new System.Threading.Tasks.TaskCompletionSource<string>(); var thread = new System.Threading.Thread(() => { try { source.TrySetResult(System.Console.In.ReadLine()); } catch (System.Exception e) { source.TrySetException(e); } }); thread.IsBackground = true; thread.Start(); return source.Task; } } }'
                }
                $ReadTask = [ZspaceLauncher.InputReader]::ReadLine()
                while (-not $ReadTask.IsCompleted -and -not $Process.WaitForExit(100)) {}
                if ($Process.HasExited) { return $Process.ExitCode }
                $AuthorizationLine = $ReadTask.GetAwaiter().GetResult()
                if ($null -ne $AuthorizationLine) {
                    $Process.StandardInput.WriteLine($AuthorizationLine)
                    $Process.StandardInput.Flush()
                }
            } finally {
                $AuthorizationLine = $null
                try { $Process.StandardInput.Close() } catch {}
            }
        }
        $Process.WaitForExit()
        return $Process.ExitCode
    } finally {
        if (-not $Process.HasExited) { $Process.Kill() }
        $Process.Dispose()
    }
}

try { Add-Type -AssemblyName System.Net.Http -ErrorAction Stop }
catch { Fail "E_RUNTIME_DEPENDENCY" ("System.Net.Http is unavailable: " + $_.Exception.Message) }

$RootDir = Split-Path -Parent $PSScriptRoot
$AllowHttp = $env:ZSPACE_RUNTIME_ALLOW_HTTP_FIXTURE -eq "1"
$ManifestPath = if ($AllowHttp -and $env:ZSPACE_RUNTIME_MANIFEST) { $env:ZSPACE_RUNTIME_MANIFEST } else { Join-Path $RootDir "runtime-artifacts.json" }
$Arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
switch ($Arch) {
    "X64" { $Platform = "windows-x64" }
    "Arm64" { $Platform = "windows-arm64" }
    default { Fail "E_UNSUPPORTED_PLATFORM" "Unsupported Windows CPU architecture" }
}

if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) { Fail "E_RUNTIME_CACHE" "Runtime artifact manifest is missing" }
try { $Manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json }
catch { Fail "E_RUNTIME_CACHE" ("Runtime artifact manifest is invalid: " + $_.Exception.Message) }
$Matches = @($Manifest.artifacts | Where-Object { $_.platform -eq $Platform })
if ($Matches.Count -ne 1) { Fail "E_UNSUPPORTED_PLATFORM" "Runtime artifact manifest does not contain this platform" }
$Artifact = $Matches[0]
$Version = [string]$Artifact.version
$Url = [string]$Artifact.url
$ExpectedSha = ([string]$Artifact.sha256).ToLowerInvariant()
$ExpectedBytes = [long]$Artifact.bytes
$FileName = [string]$Artifact.filename
if (-not $Version -or $ExpectedSha -notmatch '^[0-9a-f]{64}$' -or $ExpectedBytes -le 0 -or -not $FileName -or $FileName -match '[/\\]') { Fail "E_RUNTIME_CACHE" "Runtime artifact manifest fields are invalid" }

function Test-Artifact([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $Item = Get-Item -LiteralPath $Path -Force
    if (($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $Item.Length -ne $ExpectedBytes) { return $false }
    return ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -eq $ExpectedSha)
}

$Bundled = Join-Path (Join-Path $RootDir "bin") $FileName
if (Test-Path -LiteralPath $Bundled) {
    if (-not (Test-Artifact $Bundled)) { Fail "E_RUNTIME_INTEGRITY" "Bundled zspace-cli failed integrity verification" }
    $ResultCode = Invoke-Cli $Bundled $args
    Complete-Launcher $ResultCode
    exit $ResultCode
}

if ($AllowHttp -and $env:ZSPACE_RUNTIME_CACHE_DIR) { $CacheDir = $env:ZSPACE_RUNTIME_CACHE_DIR }
elseif ($env:LOCALAPPDATA) { $CacheDir = Join-Path $env:LOCALAPPDATA "ZSpace\AgentToolkit\cache\$Version\$Platform" }
else { Fail "E_RUNTIME_CACHE" "LOCALAPPDATA is unavailable" }
try {
    New-Item -ItemType Directory -Path $CacheDir -Force | Out-Null
    $CacheItem = Get-Item -LiteralPath $CacheDir -Force
    if (($CacheItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { Fail "E_RUNTIME_CACHE" "Runtime cache directory cannot be a reparse point" }
} catch { Fail "E_RUNTIME_CACHE" ("Unable to create runtime cache directory: " + $_.Exception.Message) }

$Cached = Join-Path $CacheDir $FileName
if (Test-Artifact $Cached) { $ResultCode = Invoke-Cli $Cached $args; Complete-Launcher $ResultCode; exit $ResultCode }
if (Test-Path -LiteralPath $Cached) {
    try { Remove-Item -LiteralPath $Cached -Force }
    catch { Fail "E_RUNTIME_CACHE" ("Unable to remove invalid runtime cache: " + $_.Exception.Message) }
}

if (-not $Url) { Fail "E_RUNTIME_DOWNLOAD" "CDN runtime artifact is not published; use the offline package" }
try { $Uri = [Uri]$Url } catch { Fail "E_RUNTIME_DOWNLOAD" "Runtime artifact URL is invalid" }
$FixtureUri = $AllowHttp -and $Uri.Scheme -eq "http" -and ($Uri.Host -eq "127.0.0.1" -or $Uri.Host -eq "localhost")
$ProductionUri = $Uri.Scheme -eq "https" -and $Uri.Host -eq "update.zenithspace.net"
if (-not $ProductionUri -and -not $FixtureUri) { Fail "E_RUNTIME_DOWNLOAD" "Runtime artifact URL is not on the trusted HTTPS CDN" }

$Temp = Join-Path $CacheDir (".zspace-cli.download." + [Guid]::NewGuid().ToString("N"))
$Handler = $null; $Client = $null; $DownloadInput = $null; $Output = $null
try {
    $Handler = New-Object Net.Http.HttpClientHandler
    $Handler.AllowAutoRedirect = $false
    $Client = New-Object Net.Http.HttpClient($Handler)
    $Client.Timeout = [TimeSpan]::FromSeconds(300)
    $Response = $Client.GetAsync($Uri, [Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
    if (-not $Response.IsSuccessStatusCode -or $Response.Headers.Location) { Fail "E_RUNTIME_DOWNLOAD" "Runtime download failed or attempted a redirect" }
    $DownloadInput = $Response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
    $Output = [IO.File]::Open($Temp, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $Buffer = New-Object byte[] 65536
    [long]$Total = 0
    while (($Read = $DownloadInput.Read($Buffer, 0, $Buffer.Length)) -gt 0) {
        $Total += $Read
        if ($Total -gt $ExpectedBytes) { Fail "E_RUNTIME_INTEGRITY" "Downloaded zspace-cli exceeds the expected size" }
        $Output.Write($Buffer, 0, $Read)
    }
    $Output.Flush($true)
    $Output.Dispose(); $Output = $null
    $DownloadInput.Dispose(); $DownloadInput = $null
    if (-not (Test-Artifact $Temp)) { Fail "E_RUNTIME_INTEGRITY" "Downloaded zspace-cli size or SHA-256 does not match" }
    try { [IO.File]::Move($Temp, $Cached) }
    catch { if (-not (Test-Artifact $Cached)) { Fail "E_RUNTIME_CACHE" ("Unable to atomically install zspace-cli: " + $_.Exception.Message) } }
} catch {
    Fail "E_RUNTIME_DOWNLOAD" ("Runtime download failed: " + $_.Exception.Message)
} finally {
    if ($Output) { $Output.Dispose() }
    if ($DownloadInput) { $DownloadInput.Dispose() }
    if ($Client) { $Client.Dispose() }
    if ($Handler) { $Handler.Dispose() }
    if (Test-Path -LiteralPath $Temp) { Remove-Item -LiteralPath $Temp -Force -ErrorAction SilentlyContinue }
}

if (-not (Test-Artifact $Cached)) { Fail "E_RUNTIME_INTEGRITY" "Installed zspace-cli failed integrity verification" }
$ResultCode = Invoke-Cli $Cached $args
Complete-Launcher $ResultCode
exit $ResultCode
