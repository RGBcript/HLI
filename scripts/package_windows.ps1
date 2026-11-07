#Requires -Version 5.1
<#
.SYNOPSIS
    Empaqueta HLI 0.9.0-beta.1 para Windows x86_64.
.DESCRIPTION
    Compila release, construye el .hlimod real, lo firma con una clave de release
    local (la privada queda fuera de dist), copia scripts y documentación, y
    genera checksums.sha256 verificados.
#>
[CmdletBinding()]
param(
    [string]$Version = "0.9.0-beta.1",
    [string]$OutDir
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $Root "dist\hli-$Version-windows-x86_64" }
$Target = Join-Path $Root "target\release"
$ModuleSrc = Join-Path $Root "modules\hli-memory"

function Get-Sha256([byte[]]$Bytes) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($Bytes)) -replace "-", "").ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Get-FileSha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Add-ContainerFile([System.Collections.Generic.List[byte]]$Out, [string]$Path, [byte[]]$Bytes) {
    $header = [Text.Encoding]::UTF8.GetBytes("FILE $($Bytes.Length) $Path`n")
    $Out.AddRange($header)
    $Out.AddRange($Bytes)
    $Out.Add([byte]10)
}

Write-Host "== build release =="
& cargo build --release --workspace --manifest-path (Join-Path $Root "Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "cargo build --release falló" }

$Bins = [ordered]@{
    "hli.exe" = "hli.exe"
    "hli-daemon.exe" = "hli-daemon.exe"
    "hli-mcp.exe" = "hli-mcp.exe"
    "hli-module-memory.exe" = "hli-memory.exe"
}
foreach ($dest in $Bins.Keys) {
    if (-not (Test-Path (Join-Path $Target $Bins[$dest]))) { throw "falta $($Bins[$dest]) en target\release" }
}

if (Test-Path $OutDir) { Remove-Item -Recurse -Force $OutDir }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
foreach ($dest in $Bins.Keys) { Copy-Item (Join-Path $Target $Bins[$dest]) (Join-Path $OutDir $dest) }

Write-Host "== módulo .hlimod =="
$manifestPath = Join-Path $ModuleSrc "manifest.json"
$manifest = Get-Content -Raw -Encoding UTF8 $manifestPath | ConvertFrom-Json
$manifest.metadata.version = $Version
$manifest.spec | Add-Member -NotePropertyName format_version -NotePropertyValue 1 -Force
$manifestBytes = [Text.Encoding]::UTF8.GetBytes(($manifest | ConvertTo-Json -Depth 20 -Compress))

$moduleBin = [IO.File]::ReadAllBytes((Join-Path $OutDir "hli-module-memory.exe"))
$files = New-Object System.Collections.Generic.List[object]
$files.Add([pscustomobject]@{ Path = "manifest.json"; Bytes = $manifestBytes })
foreach ($schema in Get-ChildItem (Join-Path $ModuleSrc "schemas") -File | Sort-Object Name) {
    $files.Add([pscustomobject]@{ Path = "schemas/$($schema.Name)"; Bytes = [IO.File]::ReadAllBytes($schema.FullName) })
}
$files.Add([pscustomobject]@{ Path = "bin/hli-memory.exe"; Bytes = $moduleBin })

$checks = [ordered]@{}
foreach ($f in $files) { $checks[$f.Path] = "sha256:" + (Get-Sha256 $f.Bytes) }
$checksBytes = [Text.Encoding]::UTF8.GetBytes(($checks | ConvertTo-Json -Compress))
$files.Add([pscustomobject]@{ Path = "checksums.json"; Bytes = $checksBytes })
$checks["checksums.json"] = "sha256:" + (Get-Sha256 $checksBytes)

$container = New-Object System.Collections.Generic.List[byte]
$container.AddRange([Text.Encoding]::UTF8.GetBytes("HLIAR1`n"))
foreach ($f in $files) { Add-ContainerFile $container $f.Path $f.Bytes }
$moduleName = "hli-memory-$Version.hlimod"
$modulePath = Join-Path $OutDir $moduleName
[IO.File]::WriteAllBytes($modulePath, $container.ToArray())

Write-Host "== firma de release =="
$signHome = Join-Path $env:TEMP ("hli-release-sign-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $signHome | Out-Null
$oldHome = $env:HLI_HOME
try {
    $env:HLI_HOME = $signHome
    $keyJson = & (Join-Path $OutDir "hli.exe") key generate release | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw "key generate falló" }
    $keyId = $keyJson.key_id
    $publicJson = & (Join-Path $OutDir "hli.exe") key export-public $keyId
    if ($LASTEXITCODE -ne 0) { throw "key export-public falló" }
    $publicPath = Join-Path $OutDir "release-public.json"
    [IO.File]::WriteAllText($publicPath, ($publicJson -join "`n"), (New-Object Text.UTF8Encoding($false)))
    & (Join-Path $OutDir "hli.exe") trust add $publicPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "trust add falló" }
    & (Join-Path $OutDir "hli.exe") module sign $modulePath --key $keyId | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "module sign falló" }
    $verified = & (Join-Path $OutDir "hli.exe") module verify $modulePath | ConvertFrom-Json
    if ($verified.state -ne "signed-trusted") { throw "firma no confiada: $($verified.state)" }
}
finally {
    $env:HLI_HOME = $oldHome
    Remove-Item -Recurse -Force $signHome -ErrorAction SilentlyContinue
}

Write-Host "== documentación y scripts =="
Copy-Item (Join-Path $PSScriptRoot "install.ps1") $OutDir
Copy-Item (Join-Path $PSScriptRoot "uninstall.ps1") $OutDir
foreach ($doc in @("README.md", "QUICKSTART.md", "LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.md")) {
    $src = Join-Path $Root $doc
    if (Test-Path $src) { Copy-Item $src $OutDir }
}
$docsOut = Join-Path $OutDir "docs"
New-Item -ItemType Directory -Force -Path $docsOut | Out-Null
foreach ($doc in Get-ChildItem (Join-Path $Root "docs") -File -Filter "*.md") {
    Copy-Item $doc.FullName $docsOut
}

Write-Host "== checksums =="
$checksumFile = Join-Path $OutDir "checksums.sha256"
$lines = New-Object System.Collections.Generic.List[string]
foreach ($file in Get-ChildItem $OutDir -Recurse -File | Sort-Object FullName) {
    if ($file.FullName -eq $checksumFile) { continue }
    $rel = $file.FullName.Substring($OutDir.Length + 1) -replace "\\", "/"
    $lines.Add(((Get-FileSha256 $file.FullName) + "  " + $rel))
}
$lines | Set-Content -Encoding ASCII $checksumFile

# Verificación final de checksums.
foreach ($line in Get-Content $checksumFile) {
    if (-not $line.Trim()) { continue }
    $parts = $line -split "  ", 2
    $actual = Get-FileSha256 (Join-Path $OutDir ($parts[1] -replace "/", "\"))
    if ($actual -ne $parts[0]) { throw "checksum no coincide: $($parts[1])" }
}

$versionLine = & (Join-Path $OutDir "hli.exe") --version
if ($versionLine.Trim() -ne "hli $Version") { throw "versión inesperada: $versionLine" }

$relative = $OutDir.Substring($Root.Length + 1)
Write-Output (@{
    package     = $relative
    version     = $Version
    files       = (Get-ChildItem $OutDir -Recurse -File).Count
    module      = $moduleName
    signed      = "signed-trusted"
    checksums   = $true
} | ConvertTo-Json -Compress)
