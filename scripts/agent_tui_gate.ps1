#Requires -Version 5.1
<#
.SYNOPSIS
    Gate automático de Agent Mode (HLI 0.9).
.DESCRIPTION
    Comprueba lo verificable sin interfaz: binarios, linking por directorio,
    creación implícita de thread/rama y run no interactivo. NO sustituye el gate
    manual de docs/AGENT-TUI-MANUAL-GATE.md.
#>
[CmdletBinding()]
param([string]$Version = "0.9.0-beta.1")

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$Result = [ordered]@{
    gate = "hli-agent-mode"; ok = $false
    binaries = $false; daemon = $false; link = $false
    implicit_thread = $false; durable = $false; run = $false; manual_pending = $true
}
$HomeDir = Join-Path $env:TEMP ("hli-agent-gate-" + [guid]::NewGuid().ToString("N"))
$Project = Join-Path $env:TEMP ("hli-agent-project-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force $HomeDir, $Project | Out-Null
$env:HLI_HOME = $HomeDir
$Hli = Join-Path $Root "target\debug\hli.exe"
$Tui = Join-Path $Root "target\debug\hli-tui.exe"

function Step($Name, [scriptblock]$Body) {
    try {
        $ok = & $Body
        Write-Host "[agent-gate] $Name = $ok"
        return [bool]$ok
    } catch {
        Write-Warning "[agent-gate] $Name falló: $($_.Exception.Message)"
        return $false
    }
}

try {
    $Result.binaries = Step "binaries" {
        (& $Hli --version) -match [regex]::Escape($Version) -and (Test-Path $Tui)
    }

    $Result.daemon = Step "daemon" {
        & $Hli daemon start | Out-Null
        $json = & $Hli daemon status --output json | ConvertFrom-Json
        $json.running -eq $true
    }

    $ws = & $Hli workspace create agent-gate --root $Project --output json | ConvertFrom-Json
    $Result.link = Step "link" { -not [string]::IsNullOrEmpty($ws.id) }

    Push-Location $Project
    try {
        $out = & $Hli run "smoke de agent mode" --output json | ConvertFrom-Json
        $Result.run = Step "run" { $out.state -in @("completed", "running") }
        $Result.implicit_thread = Step "implicit_thread" {
            $threads = & $Hli thread list --workspace $ws.id --output json | ConvertFrom-Json
            @($threads).Count -ge 1
        }
        $runs = @(& $Hli run list --workspace $ws.id --output json | ConvertFrom-Json)
        $Result.durable = Step "durable" {
            $runs.Count -ge 1 -and $null -ne $runs[-1].receipt -and
                -not [string]::IsNullOrEmpty($runs[-1].result.text)
        }
    } finally {
        Pop-Location
    }

    $Result.ok = $Result.binaries -and $Result.daemon -and $Result.link -and
        $Result.run -and $Result.implicit_thread -and $Result.durable
} catch {
    $Result.error = $_.Exception.Message
} finally {
    & $Hli daemon stop 2>$null | Out-Null
    Remove-Item -Recurse -Force $HomeDir, $Project -ErrorAction SilentlyContinue
}

$Result | ConvertTo-Json -Depth 4
Write-Host "[agent-gate] gate manual pendiente: docs/AGENT-TUI-MANUAL-GATE.md"
if (-not $Result.ok) { exit 1 }
