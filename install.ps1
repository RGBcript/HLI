#Requires -Version 5.1
<#
.SYNOPSIS
  Instala HLI en el perfil del usuario, sin privilegios de administrador.

.DESCRIPTION
  Compila los binarios en modo release y los copia a %USERPROFILE%\.local\bin
  (el mismo sitio que usa Claude Code en Windows). Añade esa carpeta al PATH
  del usuario si falta. Los cuatro binarios deben quedar juntos: `hli` busca
  `hli-tui.exe` y `hli-daemon.exe` en su propio directorio.

.PARAMETER Prefix
  Carpeta destino. Por defecto %USERPROFILE%\.local\bin

.PARAMETER Source
  Raíz del repositorio. Por defecto el directorio padre de este script.

.PARAMETER NoPath
  No toca el PATH del usuario.

.PARAMETER SkipBuild
  Usa los binarios ya compilados en target\release.

.PARAMETER Uninstall
  Elimina los binarios instalados y quita la carpeta del PATH.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\install.ps1
#>
param(
    [string]$Prefix = "$env:USERPROFILE\.local\bin",
    [string]$Source = $PSScriptRoot,
    [switch]$NoPath,
    [switch]$SkipBuild,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$Bins = @("hli.exe", "hli-tui.exe", "hli-daemon.exe", "hli-mcp.exe")

function Get-UserPath {
    $value = [Environment]::GetEnvironmentVariable("Path", "User")
    if ([string]::IsNullOrEmpty($value)) { return @() }
    return $value -split ';' | Where-Object { $_ -ne '' }
}

function Set-UserPath([string[]]$entries) {
    $joined = ($entries | Select-Object -Unique) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $joined, "User")
}

function Remove-FromPath([string]$dir) {
    $clean = Get-UserPath | Where-Object { $_.TrimEnd('\') -ne $dir.TrimEnd('\') }
    Set-UserPath $clean
    Write-Host "PATH del usuario: $dir eliminado"
}

function Add-ToPath([string]$dir) {
    $entries = Get-UserPath
    if ($entries | Where-Object { $_.TrimEnd('\') -eq $dir.TrimEnd('\') }) {
        Write-Host "PATH del usuario: ya contiene $dir"
        return
    }
    Set-UserPath ($entries + $dir)
    Write-Host "PATH del usuario: $dir añadido"
}

if ($Uninstall) {
    foreach ($b in $Bins) {
        $target = Join-Path $Prefix $b
        if (Test-Path $target) {
            Remove-Item $target -Force
            Write-Host "eliminado $target"
        }
    }
    Remove-FromPath $Prefix
    Write-Host "HLI desinstalado. Los datos de HLI_HOME no se han tocado."
    exit 0
}

if (-not (Test-Path (Join-Path $Source "Cargo.toml"))) {
    throw "No se encontró Cargo.toml en $Source. Usa -Source <ruta del repo>."
}

if (-not $SkipBuild) {
    Write-Host "Compilando HLI en release…"
    Push-Location $Source
    try {
        & cargo build --release --workspace
        if ($LASTEXITCODE -ne 0) { throw "cargo build falló con $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

$release = Join-Path $Source "target\release"
$missing = $Bins | Where-Object { -not (Test-Path (Join-Path $release $_)) }
if ($missing) { throw "Faltan binarios en ${release}: $($missing -join ', ')" }

New-Item -ItemType Directory -Force -Path $Prefix | Out-Null
foreach ($b in $Bins) {
    Copy-Item (Join-Path $release $b) (Join-Path $Prefix $b) -Force
    Write-Host "instalado $(Join-Path $Prefix $b)"
}

if (-not $NoPath) { Add-ToPath $Prefix }

# Verificación real: el binario instalado debe responder.
$version = & (Join-Path $Prefix "hli.exe") --version
if ($LASTEXITCODE -ne 0) { throw "el hli.exe instalado no responde a --version" }
Write-Host "verificado: $version"

Write-Host ""
Write-Host "Listo. Abre una terminal nueva (para heredar el PATH) y ejecuta:"
Write-Host "  hli doctor"
Write-Host "  hli"
