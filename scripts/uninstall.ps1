#Requires -Version 5.1
<#
.SYNOPSIS
    Desinstalador de HLI para Windows.
.DESCRIPTION
    Elimina binarios y el PATH añadido por HLI. Conserva HLI_HOME por defecto.
    Solo actúa sobre una instalación reconocida por su marcador.
#>
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "HLI\bin"),
    [string]$DataDir,
    [switch]$RemoveData,
    [switch]$ForceData,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$InstallDir = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
$marker = Join-Path $InstallDir ".hli-install.json"

if (-not (Test-Path $marker)) {
    if ((Test-Path (Join-Path $InstallDir "hli.exe")) -and $Force) {
        Write-Warning "Sin marcador; -Force elimina solo binarios conocidos de HLI"
    } else {
        throw "no parece una instalación HLI reconocida: $InstallDir"
    }
}

$info = $null
if (Test-Path $marker) { $info = Get-Content -Raw -Encoding UTF8 $marker | ConvertFrom-Json }
if (-not $DataDir) {
    if ($info -and $info.data_dir) { $DataDir = $info.data_dir }
    else { $DataDir = Join-Path $env:LOCALAPPDATA "HLI\data" }
}
$DataDir = [IO.Path]::GetFullPath($DataDir).TrimEnd('\')

$known = @("hli.exe", "hli-daemon.exe", "hli-mcp.exe", "hli-module-memory.exe",
           "README.md", "QUICKSTART.md", "LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.md",
           "release-public.json")
foreach ($file in Get-ChildItem $InstallDir -File -ErrorAction SilentlyContinue) {
    if ($file.Extension -eq ".hlimod") { $known += $file.Name }
}

foreach ($name in $known) {
    $path = Join-Path $InstallDir $name
    if (Test-Path $path) { Remove-Item -Force $path }
}

if ($info -and $info.path_added) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath) {
        $parts = $userPath -split ";" | Where-Object { $_ -and ($_ -ine $InstallDir) }
        [Environment]::SetEnvironmentVariable("Path", ($parts -join ";"), "User")
    }
}

$remaining = Get-ChildItem $InstallDir -Force -ErrorAction SilentlyContinue
if (-not $remaining) { Remove-Item -Force $InstallDir -ErrorAction SilentlyContinue }

if ($RemoveData) {
    $confirmed = $ForceData
    if (-not $confirmed) {
        if ([Environment]::UserInteractive -and -not [Console]::IsInputRedirected) {
            $answer = Read-Host "Esto borrará TODOS los datos de HLI en $DataDir. Escribe BORRAR para continuar"
            $confirmed = ($answer -eq "BORRAR")
        } else {
            throw "RemoveData en modo no interactivo requiere -ForceData"
        }
    }
    if (-not $confirmed) { throw "borrado de datos cancelado" }
    $root = [IO.Path]::GetPathRoot($DataDir).TrimEnd('\')
    $forbidden = @($root, $env:USERPROFILE, $env:LOCALAPPDATA, $env:APPDATA,
                   $env:ProgramFiles, $env:WINDIR)
    foreach ($f in $forbidden) {
        if ($f -and ($DataDir -ieq ([IO.Path]::GetFullPath($f).TrimEnd('\')))) {
            throw "DataDir inseguro: $DataDir"
        }
    }
    if (Test-Path $DataDir) { Remove-Item -Recurse -Force $DataDir }
    Write-Output "Datos eliminados: $DataDir"
} else {
    Write-Output "Datos conservados en: $DataDir"
}

if (Test-Path $marker) { Remove-Item -Force $marker }
Write-Output "HLI desinstalado de $InstallDir"
