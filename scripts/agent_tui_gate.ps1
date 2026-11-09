#Requires -Version 5.1
<#
.SYNOPSIS
    Gate automático consolidado de Agent Mode (HLI 0.9).
.DESCRIPTION
    Verifica binarios y versión, y delega el recorrido completo en
    scripts/agent_tui_gate.py (linking, plan, permisos, revisión, referencias,
    transcript, resume, Inspector, async/cancelación, receipt, artefactos,
    JSONL, MCP, MCH y secret safety). El campo manual_pending permanece true:
    este gate NO sustituye docs/AGENT-TUI-MANUAL-GATE.md.
#>
[CmdletBinding()]
param([string]$Version = "0.9.0-beta.1")

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$Hli = Join-Path $Root "target\debug\hli.exe"
$Daemon = Join-Path $Root "target\debug\hli-daemon.exe"
$Mcp = Join-Path $Root "target\debug\hli-mcp.exe"

function Fail([string]$Message) {
    [ordered]@{
        gate = "agent-tui"; ok = $false; manual_pending = $true; error = $Message
        workspace_linking = $false; durable_thread = $false; planning = $false
        plan_approval = $false; tool_permissions = $false; plan_revision = $false
        explicit_references = $false; activity = $false; transcript = $false
        resume = $false; inspector = $false; async_run = $false
        cancellation = $false; receipt = $false; artifacts = $false
        jsonl_follow = $false; mcp = $false; mch = $false; secret_safety = $false
    } | ConvertTo-Json -Depth 4
    exit 1
}

foreach ($bin in @($Hli, $Daemon, $Mcp)) {
    if (-not (Test-Path $bin)) { Fail "falta el binario $bin; ejecuta cargo build --workspace" }
}
$reported = (& $Hli --version) -join " "
if ($reported -notmatch [regex]::Escape($Version)) {
    Fail "versión inesperada: '$reported' (se esperaba $Version)"
}

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) { $python = Get-Command python3 -ErrorAction SilentlyContinue }
if (-not $python) { Fail "python no está disponible para ejecutar scripts/agent_tui_gate.py" }

& $python.Source "scripts/agent_tui_gate.py"
$code = $LASTEXITCODE
Write-Host "[agent-gate] gate manual pendiente: docs/AGENT-TUI-MANUAL-GATE.md"
exit $code
