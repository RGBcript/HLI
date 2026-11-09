#Requires -Version 5.1
<#
.SYNOPSIS
    Gate de referencias explícitas. Delegado en Python por estabilidad de
    redirecciones; los comandos y comprobaciones son los mismos.
#>
$Root = Split-Path -Parent $PSScriptRoot
& python (Join-Path $Root "scripts\explicit_reference_gate.py")
exit $LASTEXITCODE
