#Requires -Version 5.1
<#
.SYNOPSIS
    Gate de desplazamiento y Transcript Mode (HLI 0.9).
.DESCRIPTION
    Delega en scripts/agent_scroll_gate.py: pruebas de modelo de viewport,
    eventos de ratón inyectados, scrollbar, follow, búsqueda y verificación de
    la captura/restauración del ratón. No sustituye el gate humano.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) { $python = Get-Command python3 -ErrorAction SilentlyContinue }
if (-not $python) {
    Write-Error "python no está disponible para ejecutar scripts/agent_scroll_gate.py"
    exit 1
}

& $python.Source "scripts/agent_scroll_gate.py"
exit $LASTEXITCODE
