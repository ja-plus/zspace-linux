param(
    [string]$Executable,
    [string]$Launcher = (Join-Path $PSScriptRoot "zspace-cli.ps1")
)
$ErrorActionPreference = "Stop"
$Launcher = (Resolve-Path $Launcher).Path
$RootDir = Split-Path -Parent $PSScriptRoot
$Platform = "windows-$([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant())"
if (-not $Executable) {
    $Runtime = Get-Content -LiteralPath (Join-Path $RootDir "runtime-artifacts.json") -Raw | ConvertFrom-Json
    $Artifact = @($Runtime.artifacts | Where-Object { $_.platform -eq $Platform })[0]
    if (-not $Artifact) { throw "runtime artifact not found for $Platform" }
    $Executable = Join-Path $env:LOCALAPPDATA "ZSpace\AgentToolkit\cache\$($Artifact.version)\$Platform\$($Artifact.filename)"
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        & powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $Launcher --version | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "unable to populate the verified runtime cache" }
    }
}
$Executable = (Resolve-Path $Executable).Path
$Python = Get-Command py,python,python3 -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $Python) { throw "Python is required only for the loopback self-test server (py, python, or python3)" }
$Temp = Join-Path ([IO.Path]::GetTempPath()) ("zspace-runtime-selftest-" + [Guid]::NewGuid().ToString("N"))
$Server = $null
try {
    New-Item -ItemType Directory -Path $Temp | Out-Null
    $Serve = Join-Path $Temp "serve"; $Cache = Join-Path $Temp "cache"
    New-Item -ItemType Directory -Path $Serve | Out-Null
    $VersionText = (& $Executable --version).Trim()
    if ($LASTEXITCODE -ne 0 -or $VersionText -notmatch '^zspace-cli (.+)$') { throw "invalid executable version" }
    $Version = $Matches[1]
    $FileName = "zspace-cli-v$Version-$Platform.exe"
    $Served = Join-Path $Serve $FileName
    Copy-Item $Executable $Served
    $Sha = (Get-FileHash $Served -Algorithm SHA256).Hash.ToLowerInvariant()
    $Bytes = (Get-Item $Served).Length
    $Listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0); $Listener.Start(); $Port = $Listener.LocalEndpoint.Port; $Listener.Stop()
    $PythonArgs = if ($Python.Name -eq 'py.exe' -or $Python.Name -eq 'py') { @('-3') } else { @() }
    $PythonArgs += @('-m','http.server',$Port,'--bind','127.0.0.1','--directory',$Serve)
    $Server = Start-Process $Python.Source -ArgumentList $PythonArgs -PassThru -WindowStyle Hidden
    Start-Sleep -Milliseconds 700
    $Manifest = Join-Path $Temp "fixture.json"
    @{version=$Version; published=$true; artifacts=@(@{platform=$Platform; version=$Version; url="http://127.0.0.1:$Port/$FileName"; sha256=$Sha; bytes=$Bytes; filename=$FileName})} | ConvertTo-Json -Depth 5 -Compress | Set-Content -LiteralPath $Manifest -Encoding UTF8
    $env:ZSPACE_RUNTIME_MANIFEST=$Manifest; $env:ZSPACE_RUNTIME_CACHE_DIR=$Cache; $env:ZSPACE_RUNTIME_ALLOW_HTTP_FIXTURE='1'
    & powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $Launcher --version | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "initial fixture download failed" }
    Stop-Process -Id $Server.Id -Force; $Server=$null
    & powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $Launcher --version | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "verified cache execution failed" }

    # Compile a local-only native probe so this test can prove stdin forwarding
    # and prompt exit without contacting NAS/OAuth or exposing an authorization code.
    $Probe = Join-Path $Serve "stdin-probe.exe"
    Add-Type -TypeDefinition @'
using System;
public static class StdinProbe {
    public static int Main(string[] args) {
        if (Array.IndexOf(args, "--exit-without-input") >= 0) {
            System.Threading.Thread.Sleep(500);
            Console.WriteLine("EARLY_EXIT");
            return 19;
        }
        if (Array.IndexOf(args, "--auth-code-stdin") >= 0) {
            string line = Console.ReadLine();
            Console.WriteLine("STDIN=" + (line ?? "<EOF>"));
            return line == "selftest-code" ? 37 : 38;
        }
        Console.WriteLine("NO_STDIN");
        return 23;
    }
}
'@ -Language CSharp -OutputAssembly $Probe -OutputType ConsoleApplication
    $ProbeSha = (Get-FileHash $Probe -Algorithm SHA256).Hash.ToLowerInvariant()
    $ProbeBytes = (Get-Item $Probe).Length
    $ProbeName = "zspace-cli-v$Version-$Platform.exe"
    Copy-Item -LiteralPath $Probe -Destination (Join-Path $Serve $ProbeName) -Force
    @{version=$Version; published=$true; artifacts=@(@{platform=$Platform; version=$Version; url="http://127.0.0.1:$Port/$ProbeName"; sha256=$ProbeSha; bytes=$ProbeBytes; filename=$ProbeName})} | ConvertTo-Json -Depth 5 -Compress | Set-Content -LiteralPath $Manifest -Encoding UTF8
    $env:ZSPACE_RUNTIME_CACHE_DIR = Join-Path $Temp "probe-cache"
    New-Item -ItemType Directory -Path $env:ZSPACE_RUNTIME_CACHE_DIR -Force | Out-Null
    Copy-Item -LiteralPath $Probe -Destination (Join-Path $env:ZSPACE_RUNTIME_CACHE_DIR $ProbeName)

    function Invoke-LauncherProbe([string[]]$Arguments, [string]$InputLine, [int]$ExpectedExit, [string]$ExpectedOutput) {
        $Info = New-Object Diagnostics.ProcessStartInfo
        $Info.FileName = "powershell.exe"
        $QuotedLauncher = '"' + $Launcher.Replace('"','\"') + '"'
        $Info.Arguments = "-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $QuotedLauncher " + ($Arguments -join ' ')
        $Info.UseShellExecute = $false
        $Info.RedirectStandardInput = $true
        $Info.RedirectStandardOutput = $true
        $Info.RedirectStandardError = $true
        $Child = [Diagnostics.Process]::Start($Info)
        if ($InputLine) { $Child.StandardInput.WriteLine($InputLine); $Child.StandardInput.Flush() }
        # Deliberately keep stdin open: both probes must finish without waiting for EOF.
        if (-not $Child.WaitForExit(10000)) { try { $Child.Kill() } catch {}; if ($InputLine) { throw "probe child blocked on stdin: authorization line was not delivered" } else { throw "launcher waited for stdin EOF after ordinary command" } }
        $Out = $Child.StandardOutput.ReadToEnd().Trim()
        $Err = $Child.StandardError.ReadToEnd()
        if ($Child.ExitCode -ne $ExpectedExit -or $Out -ne $ExpectedOutput) {
            throw "launcher probe failed: exit=$($Child.ExitCode), stdout=$Out, stderr=$Err"
        }
    }
    Invoke-LauncherProbe @('--auth-code-stdin') 'selftest-code' 37 'STDIN=selftest-code'
    Invoke-LauncherProbe @('--version') $null 23 'NO_STDIN'
    Invoke-LauncherProbe @('--auth-code-stdin','--exit-without-input') $null 19 'EARLY_EXIT'
    @{ok=$true; passed=5; version=$Version; platform=$Platform; stdin_forwarding='passed'; no_eof_wait='passed'; child_exit_without_input='passed'} | ConvertTo-Json -Compress
} finally {
    if ($Server) { Stop-Process -Id $Server.Id -Force -ErrorAction SilentlyContinue }
    Remove-Item Env:ZSPACE_RUNTIME_MANIFEST,Env:ZSPACE_RUNTIME_CACHE_DIR,Env:ZSPACE_RUNTIME_ALLOW_HTTP_FIXTURE -ErrorAction SilentlyContinue
    if (Test-Path $Temp) { Remove-Item $Temp -Recurse -Force }
}
