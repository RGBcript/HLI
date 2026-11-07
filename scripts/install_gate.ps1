#Requires -Version 5.1
<#
.SYNOPSIS
    Gate de instalación limpia sobre el paquete Windows real.
#>
[CmdletBinding()]
param(
    [string]$Version = "0.9.0-beta.1",
    [string]$OutFile
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$PackageDir = Join-Path $Root "dist\hli-$Version-windows-x86_64"
$Base = Join-Path $env:TEMP ("hli-install-gate-" + [guid]::NewGuid().ToString("N"))
$InstallDir = Join-Path $Base "app"
$DataDir = Join-Path $Base "data"
$DataDir2 = Join-Path $Base "data-import"
$Exe = Join-Path $InstallDir "hli.exe"
$Result = [ordered]@{
    gate = "windows-install"; ok = $false; checksums = $false; install = $false
    version = $false; daemon = $false; run = $false; mch_memory = $false
    restart = $false; export_import = $false; uninstall_preserved_data = $false
    reinstall = $false
}
$env:HLI_HOME = $DataDir

function Get-FileSha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Test-PackageChecksums {
    $file = Join-Path $PackageDir "checksums.sha256"
    foreach ($line in Get-Content $file) {
        if (-not $line.Trim()) { continue }
        $parts = $line -split "  ", 2
        $target = Join-Path $PackageDir ($parts[1] -replace "/", "\")
        if (-not (Test-Path $target)) { return $false }
        if ((Get-FileSha256 $target) -ne $parts[0]) { return $false }
    }
    $true
}

function Invoke-Hli {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    $out = & $Exe @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "hli $($Arguments -join ' ') falló: $($out -join ' ')" }
    return ($out -join "`n")
}

function Invoke-HliJson {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
    return ((Invoke-Hli @Arguments) | ConvertFrom-Json)
}

function Invoke-HliDaemon([string]$Action) {
    $log = Join-Path $Base "cli-daemon-$Action.log"
    $line = "`"$Exe`" daemon $Action > `"$log`" 2>&1"
    & cmd.exe /c $line
    if ($LASTEXITCODE -ne 0) {
        $detail = ""
        if (Test-Path $log) { $detail = Get-Content -Raw $log }
        throw "daemon $Action falló: $detail"
    }
}

function Wait-Daemon {
    for ($i = 0; $i -lt 100; $i++) {
        Start-Sleep -Milliseconds 100
        try {
            $status = & $Exe daemon status 2>$null | ConvertFrom-Json
            if ($status.running) { return $true }
        } catch { }
    }
    return $false
}

try {
    if (-not (Test-Path $PackageDir)) {
        & (Join-Path $PSScriptRoot "package_windows.ps1") -Version $Version | Out-Null
    }
    Write-Host "[gate] checksums"
    $Result.checksums = Test-PackageChecksums
    if (-not $Result.checksums) { throw "checksums del paquete inválidos" }

    $pathBefore = [Environment]::GetEnvironmentVariable("Path", "User")
    Write-Host "[gate] install"
    & (Join-Path $PackageDir "install.ps1") -InstallDir $InstallDir -DataDir $DataDir -NoPath -Force | Out-Null
    $Result.install = (Test-Path $Exe) -and ((Test-Path (Join-Path $InstallDir "hli-daemon.exe")))
    $pathAfter = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($pathBefore -ne $pathAfter) { throw "install -NoPath modificó el PATH del usuario" }

    $versionLine = (& $Exe --version)
    $Result.version = ($versionLine.Trim() -eq "hli $Version")
    if (-not $Result.version) { throw "versión inesperada: $versionLine" }

    Write-Host "[gate] daemon start"
    Invoke-HliDaemon start
    if (-not (Wait-Daemon)) { throw "daemon no arrancó" }
    $Result.daemon = $true

    Write-Host "[gate] workspace/run"
    $ws = (Invoke-HliJson workspace create gate).id
    $thread = (Invoke-HliJson thread create gate-thread --workspace $ws).id
    $branch = (Invoke-HliJson branch create main --thread $thread).id
    Invoke-Hli provider add sim1 --kind sim --base-url "http://localhost" --model "sim-1" | Out-Null
    Invoke-Hli provider set-default sim1 | Out-Null
    $run = Invoke-HliJson run start --workspace $ws --thread $thread --branch $branch --prompt "gate de instalación" --provider sim1
    $Result.run = ($run.state -eq "completed") -and ($null -ne $run.receipt)
    if (-not $Result.run) { throw "el run no completó" }

    Write-Host "[gate] module/mch/memory"
    Invoke-Hli trust add (Join-Path $PackageDir "release-public.json") | Out-Null
    $module = Get-ChildItem $PackageDir -Filter "*.hlimod" | Select-Object -First 1
    $installed = Invoke-HliJson module install $module.FullName
    if ($installed.signature_state -ne "signed-trusted") { throw "módulo no firmado/confiado: $($installed.signature_state)" }
    Invoke-Hli mch create memory-main --module hli-memory --workspace $ws | Out-Null
    Invoke-Hli mch serve memory-main | Out-Null
    Invoke-Hli memory commit --workspace $ws --type fact --content "memoria del gate de instalación" | Out-Null
    $found = Invoke-HliJson memory search --workspace $ws --query "gate"
    $mch = Invoke-HliJson mch invoke memory-main memory.search --arguments '{"query":""}'
    $mchStatus = Invoke-HliJson mch status memory-main
    $Result.mch_memory = ($mchStatus.process.state -eq "ready") -and ($null -ne $mch.result) -and (($found | ConvertTo-Json -Depth 8) -match "memoria del gate")
    if (-not $Result.mch_memory) { throw "MCH/memoria no respondió" }

    Write-Host "[gate] restart"
    Invoke-HliDaemon stop
    Start-Sleep -Milliseconds 300
    Invoke-HliDaemon start
    if (-not (Wait-Daemon)) { throw "daemon no reinició" }
    $persisted = Invoke-HliJson memory search --workspace $ws --query "gate"
    $Result.restart = (($persisted | ConvertTo-Json -Depth 8) -match "memoria del gate")
    if (-not $Result.restart) { throw "la memoria no persistió" }

    Write-Host "[gate] export"
    $archive = Join-Path $Base "gate.hliar"
    Invoke-Hli export $ws --out $archive | Out-Null

    Invoke-HliDaemon stop
    Start-Sleep -Milliseconds 300
    Write-Host "[gate] uninstall"
    & (Join-Path $PackageDir "uninstall.ps1") -InstallDir $InstallDir | Out-Null
    $Result.uninstall_preserved_data = (-not (Test-Path $Exe)) -and (Test-Path (Join-Path $DataDir "hli.db"))
    if (-not $Result.uninstall_preserved_data) { throw "uninstall no preservó datos" }

    & (Join-Path $PackageDir "install.ps1") -InstallDir $InstallDir -DataDir $DataDir -NoPath -Force | Out-Null
    $Result.reinstall = (Test-Path $Exe)
    if (-not $Result.reinstall) { throw "reinstalación falló" }

    Write-Host "[gate] reinstall/import"
    New-Item -ItemType Directory -Force -Path $DataDir2 | Out-Null
    $env:HLI_HOME = $DataDir2
    Invoke-HliDaemon start
    if (-not (Wait-Daemon)) { throw "daemon de import no arrancó" }
    Invoke-Hli import $archive | Out-Null
    $verify = Invoke-HliJson verify --workspace $ws
    $imported = Invoke-HliJson memory search --workspace $ws --query "gate"
    $Result.export_import = ($verify.ok -eq $true) -and (($imported | ConvertTo-Json -Depth 8) -match "memoria del gate")
    if (-not $Result.export_import) { throw "export/import no equivalente" }
    Invoke-HliDaemon stop

    $Result.ok = $Result.checksums -and $Result.install -and $Result.version -and
        $Result.daemon -and $Result.run -and $Result.mch_memory -and $Result.restart -and
        $Result.export_import -and $Result.uninstall_preserved_data -and $Result.reinstall
}
catch {
    $Result.error = $_.Exception.Message
}
finally {
    $env:HLI_HOME = $DataDir
    try { Invoke-HliDaemon stop } catch { }
    Remove-Item -Recurse -Force $Base -ErrorAction SilentlyContinue
}

$json = $Result | ConvertTo-Json -Depth 5
$json
if ($OutFile) { [IO.File]::WriteAllText($OutFile, $json, (New-Object Text.UTF8Encoding($false))) }
if (-not $Result.ok) { exit 1 }
