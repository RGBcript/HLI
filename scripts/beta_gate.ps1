#Requires -Version 5.1
<#
.SYNOPSIS
    Gate general de la Beta 0.9: pruebas, seguridad, gates y distribución.
.DESCRIPTION
    Ejecuta la batería completa y produce un JSON. `ok` exige también el gate
    visual manual, que no se automatiza: se marca false hasta que el usuario lo
    firme en docs/TUI-MANUAL-GATE.md.
#>
[CmdletBinding()]
param(
    [string]$Version = "0.9.0-beta.1",
    [switch]$KeepLogs
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Result = [ordered]@{
    gate = "hli-0.9-beta"; ok = $false; tests = $false; security = $false
    mcp = $false; modules = $false; mch_supervisor = $false; signing = $false
    migrations = $false; packaging = $false; installation = $false; quickstart = $false
    tui_manual = $false
}
$LogDir = Join-Path $env:TEMP ("hli-beta-gate-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Invoke-Step {
    param([string]$Name, [scriptblock]$Body, [switch]$AllowFail)
    $log = Join-Path $LogDir ("$Name.log")
    Write-Host "[beta] $Name"
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $Body 1> $log
        $code = $LASTEXITCODE
        if ($null -eq $code) { $code = 0 }
        if ($code -ne 0 -and -not $AllowFail) {
            Write-Warning "$Name falló (exit $code); ver $log"
            return $false
        }
        return ($code -eq 0)
    } catch {
        Write-Warning "$Name falló: $($_.Exception.Message)"
        return $false
    } finally {
        $ErrorActionPreference = $previous
    }
}

try {
    $fmt = Invoke-Step "fmt" { & cargo fmt --all -- --check }
    $clippy = Invoke-Step "clippy" { & cargo clippy --workspace --all-targets --all-features -- -D warnings }
    $debug = Invoke-Step "tests-debug" { & cargo test --workspace }
    $release = Invoke-Step "tests-release" { & cargo test --workspace --release }
    $Result.tests = $fmt -and $clippy -and $debug -and $release

    $audit = Invoke-Step "audit" { & cargo audit }
    $deny = Invoke-Step "deny" { & cargo deny check }
    $Result.security = $audit -and $deny

    $Result.mcp = Invoke-Step "mcp-gate" { & python scripts/mcp_gate.py }
    $Result.modules = Invoke-Step "module-gate" { & python scripts/module_gate.py }
    $Result.mch_supervisor = Invoke-Step "mch-supervisor-gate" { & python scripts/mch_supervisor_gate.py }
    $Result.signing = Invoke-Step "signing-gate" { & python scripts/module_signing_gate.py }
    $Result.migrations = Invoke-Step "migration-gate" { & python scripts/migration_gate.py }

    $pkg = Invoke-Step "package" { & (Join-Path $PSScriptRoot "package_windows.ps1") -Version $Version }
    $packageDir = Join-Path $Root "dist\hli-$Version-windows-x86_64"
    $Result.packaging = $pkg -and (Test-Path (Join-Path $packageDir "checksums.sha256"))

    $gateFile = Join-Path $LogDir "install-gate.json"
    $installLog = Join-Path $LogDir "install-gate.log"
    $installErr = Join-Path $LogDir "install-gate.err"
    Write-Host "[beta] install-gate"
    $install = $false
    $proc = Start-Process -FilePath "powershell.exe" -ArgumentList @(
        "-NoProfile", "-ExecutionPolicy", "Bypass",
        "-File", (Join-Path $PSScriptRoot "install_gate.ps1"),
        "-Version", $Version, "-OutFile", $gateFile
    ) -NoNewWindow -PassThru -RedirectStandardOutput $installLog -RedirectStandardError $installErr
    if ($proc.WaitForExit(600000)) {
        $install = ($proc.ExitCode -eq 0)
    } else {
        $proc.Kill()
    }
    Set-Content -Path (Join-Path $LogDir "install-exit.txt") -Value ("exit=" + $proc.ExitCode + " gateFile=" + (Test-Path $gateFile))
    if (Test-Path $gateFile) {
        $installJson = Get-Content -Raw -Encoding UTF8 $gateFile | ConvertFrom-Json
        $Result.installation = ($installJson.ok -eq $true)
        # El Quickstart automatizado es el mismo flujo instalado: versión, daemon,
        # run, módulo firmado, MCH/memoria, reinicio y export/import.
        $Result.quickstart = $Result.installation -and ($installJson.run -eq $true) -and
            ($installJson.mch_memory -eq $true) -and ($installJson.restart -eq $true)
    }

    $Result.tui_manual = $false
    $Result.ok = $Result.tests -and $Result.security -and $Result.mcp -and $Result.modules -and
        $Result.mch_supervisor -and $Result.signing -and $Result.migrations -and
        $Result.packaging -and $Result.installation -and $Result.quickstart -and
        $Result.tui_manual
}
catch {
    $Result.error = $_.Exception.Message
}
finally {
    if ($KeepLogs) { Write-Host "[beta] logs: $LogDir" }
    else { Remove-Item -Recurse -Force $LogDir -ErrorAction SilentlyContinue }
}

$Result | ConvertTo-Json -Depth 5
if (-not $Result.ok) { exit 1 }
