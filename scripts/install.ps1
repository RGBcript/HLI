#Requires -Version 5.1
<#
.SYNOPSIS
    Instalador por usuario de HLI 0.9.0-beta.1.
.DESCRIPTION
    Verifica checksums, copia binarios y documentación, crea datos sin secretos y
    opcionalmente añade el bin al PATH del usuario. Idempotente y con rollback de
    los cambios de la ejecución si algo falla.
#>
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "HLI\bin"),
    [string]$DataDir = (Join-Path $env:LOCALAPPDATA "HLI\data"),
    [switch]$NoPath,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$PackageDir = $PSScriptRoot
$Version = "0.9.0-beta.1"
$Binaries = @("hli.exe", "hli-daemon.exe", "hli-mcp.exe", "hli-module-memory.exe")
$Docs = @("README.md", "QUICKSTART.md", "LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.md")
$createdFiles = New-Object System.Collections.Generic.List[string]
$createdDirs = New-Object System.Collections.Generic.List[string]
$backupDir = Join-Path $env:TEMP ("hli-install-backup-" + [guid]::NewGuid().ToString("N"))
$backups = @{}
$pathChanged = $false
$oldPath = $null

function Copy-Tracked([string]$Source, [string]$Destination) {
    if (-not (Test-Path $backupDir)) { New-Item -ItemType Directory -Force -Path $backupDir | Out-Null }
    if (Test-Path $Destination) {
        $key = [guid]::NewGuid().ToString("N")
        Copy-Item -Force $Destination (Join-Path $backupDir $key)
        $script:backups[$Destination] = (Join-Path $backupDir $key)
    } else {
        $script:createdFiles.Add($Destination)
    }
    Copy-Item -Force $Source $Destination
}

function Get-FileSha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Assert-SafeDir([string]$Path, [string]$Name) {
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $root = [IO.Path]::GetPathRoot($full).TrimEnd('\')
    $forbidden = @($root, $env:USERPROFILE, $env:LOCALAPPDATA, $env:APPDATA,
                   $env:ProgramFiles, ${env:ProgramFiles(x86)}, $env:WINDIR)
    foreach ($f in $forbidden) {
        if ($f -and ($full -ieq ([IO.Path]::GetFullPath($f).TrimEnd('\')))) {
            throw "$Name inseguro: $full"
        }
    }
    if ($full.Length -le ($root.Length + 2)) { throw "$Name demasiado superficial: $full" }
    return $full
}

function Test-Checksums {
    $file = Join-Path $PackageDir "checksums.sha256"
    if (-not (Test-Path $file)) { throw "falta checksums.sha256" }
    foreach ($line in Get-Content $file) {
        if (-not $line.Trim()) { continue }
        $parts = $line -split "  ", 2
        $target = Join-Path $PackageDir ($parts[1] -replace "/", "\")
        if (-not (Test-Path $target)) { throw "falta $($parts[1])" }
        if ((Get-FileSha256 $target) -ne $parts[0]) { throw "checksum inválido: $($parts[1])" }
    }
}

try {
    $InstallDir = Assert-SafeDir $InstallDir "InstallDir"
    $DataDir = Assert-SafeDir $DataDir "DataDir"
    Test-Checksums

    $marker = Join-Path $InstallDir ".hli-install.json"
    $existing = (Test-Path (Join-Path $InstallDir "hli.exe")) -or (Test-Path $marker)
    if ($existing -and -not $Force) {
        throw "ya existe una instalación en $InstallDir; usa -Force para sobrescribir"
    }

    foreach ($dir in @($InstallDir, $DataDir)) {
        if (-not (Test-Path $dir)) {
            New-Item -ItemType Directory -Force -Path $dir | Out-Null
            $createdDirs.Add($dir)
        }
    }
    foreach ($sub in @("artifacts", "snapshots", "backups")) {
        $p = Join-Path $DataDir $sub
        if (-not (Test-Path $p)) { New-Item -ItemType Directory -Force -Path $p | Out-Null; $createdDirs.Add($p) }
    }

    $copied = New-Object System.Collections.Generic.List[string]
    foreach ($bin in $Binaries) {
        $src = Join-Path $PackageDir $bin
        if (-not (Test-Path $src)) { throw "el paquete no contiene $bin" }
        $dst = Join-Path $InstallDir $bin
        Copy-Tracked $src $dst
        $copied.Add($dst)
        if (-not (Test-Path $dst)) { throw "no se copió $bin" }
    }
    foreach ($doc in $Docs) {
        $src = Join-Path $PackageDir $doc
        if (Test-Path $src) {
            $dst = Join-Path $InstallDir $doc
            Copy-Tracked $src $dst
            $copied.Add($dst)
        }
    }
    $module = Get-ChildItem $PackageDir -Filter "*.hlimod" | Select-Object -First 1
    if (-not $module) { throw "el paquete no contiene .hlimod" }
    Copy-Tracked $module.FullName (Join-Path $InstallDir $module.Name)
    $copied.Add((Join-Path $InstallDir $module.Name))
    $public = Join-Path $PackageDir "release-public.json"
    if (Test-Path $public) { Copy-Tracked $public (Join-Path $InstallDir "release-public.json") }

    $policyFile = Join-Path $DataDir "module-policy.json"
    if (-not (Test-Path $policyFile)) {
        '{"version":1,"policy":"warn-unsigned"}' | Set-Content -Encoding UTF8 $policyFile
        $createdFiles.Add($policyFile)
    }

    if (-not $NoPath) {
        $oldPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $parts = @()
        if ($oldPath) { $parts = $oldPath -split ";" | Where-Object { $_ } }
        if (-not ($parts | Where-Object { $_ -ieq $InstallDir })) {
            $newPath = (($parts + $InstallDir) -join ";")
            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
            $pathChanged = $true
        }
    }

    $markerBody = @{
        version = $Version
        install_dir = $InstallDir
        data_dir = $DataDir
        path_added = (-not $NoPath)
        installed_at = (Get-Date).ToUniversalTime().ToString("o")
        files = $copied
    } | ConvertTo-Json -Depth 5
    $markerBody | Set-Content -Encoding UTF8 $marker
    $createdFiles.Add($marker)

    $exe = Join-Path $InstallDir "hli.exe"
    $versionLine = & $exe --version
    if ($versionLine.Trim() -ne "hli $Version") { throw "versión inesperada: $versionLine" }
    if (Test-Path $backupDir) { Remove-Item -Recurse -Force $backupDir -ErrorAction SilentlyContinue }

    Write-Output "HLI $Version instalado en $InstallDir"
    Write-Output "Datos: $DataDir"
    Write-Output "Siguiente paso: `$env:HLI_HOME='$DataDir'; & '$exe' daemon start"
}
catch {
    foreach ($f in $createdFiles) { Remove-Item -Force $f -ErrorAction SilentlyContinue }
    foreach ($target in $backups.Keys) {
        Copy-Item -Force $backups[$target] $target -ErrorAction SilentlyContinue
    }
    foreach ($d in $createdDirs) { Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue }
    if ($pathChanged) {
        [Environment]::SetEnvironmentVariable("Path", $oldPath, "User")
    }
    if (Test-Path $backupDir) { Remove-Item -Recurse -Force $backupDir -ErrorAction SilentlyContinue }
    throw
}
