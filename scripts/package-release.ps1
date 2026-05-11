param(
    [string]$Target = "x86_64-pc-windows-gnu",
    [string]$OutputRoot = "dist",
    [switch]$RefreshChecksums,
    [switch]$SkipStandaloneGrubBuild,
    [switch]$SkipProvenanceCheck
)

$ErrorActionPreference = "Stop"

function Get-Crc32Hex([string]$Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $crc = 0xFFFFFFFFL
    foreach ($b in $bytes) {
        $crc = ($crc -bxor [int64]$b) -band 0xFFFFFFFFL
        for ($i = 0; $i -lt 8; $i++) {
            if (($crc -band 1L) -eq 1L) {
                $crc = ((($crc -shr 1) -bxor 0xEDB88320L) -band 0xFFFFFFFFL)
            } else {
                $crc = (($crc -shr 1) -band 0xFFFFFFFFL)
            }
        }
    }
    $crc = (-bnot $crc) -band 0xFFFFFFFFL
    return ("{0:X8}" -f $crc)
}

function Get-ChecksumsMap([string]$ManifestPath) {
    $map = @{}
    foreach ($line in Get-Content $ManifestPath) {
        $trimmed = $line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith("#")) {
            continue
        }
        $parts = $trimmed.Split("=", 2)
        if ($parts.Length -ne 2) {
            throw "Invalid checksum line in ${ManifestPath}: $trimmed"
        }
        $name = $parts[0].Trim()
        $value = $parts[1].Trim().ToUpperInvariant()
        if ($value.Length -ne 8 -or ($value -notmatch "^[0-9A-F]+$")) {
            throw "Invalid checksum value for $name in ${ManifestPath}: $value"
        }
        $map[$name.ToLowerInvariant()] = $value
    }
    return $map
}

function Assert-ProvenanceComplete([string]$ProvenancePath) {
    if (!(Test-Path $ProvenancePath)) {
        throw "Missing provenance document: $ProvenancePath"
    }

    $content = Get-Content $ProvenancePath -Raw
    $releaseSectionPattern = "(?s)##\s*Current release record.*?(?=^##\s|\z)"
    $releaseSection = [System.Text.RegularExpressions.Regex]::Match(
        $content,
        $releaseSectionPattern,
        [System.Text.RegularExpressions.RegexOptions]::Multiline
    ).Value
    if ([string]::IsNullOrWhiteSpace($releaseSection)) {
        throw "Could not find 'Current release record' section in $ProvenancePath"
    }

    if ($releaseSection -match "Status:\s*\*\*provisional\*\*" -or $releaseSection -match "Unknown \(to be confirmed\)") {
        throw "EFI provenance is incomplete in ${ProvenancePath}. Fill all source ISO/hash/signing fields before packaging."
    }
}

function Assert-EfiSignatureMetadata([string]$Path, [bool]$RequireSigned = $true) {
    $signature = Get-AuthenticodeSignature $Path
    if ($signature.Status -eq "NotSigned") {
        if ($RequireSigned) {
            throw "EFI binary is unsigned: $Path"
        }
        Write-Host "[warn] EFI binary is unsigned (allowed): $Path"
        return
    }
    if ($null -eq $signature.SignerCertificate) {
        throw "EFI binary signer certificate metadata missing: $Path"
    }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cargoToml = Join-Path $repoRoot "Cargo.toml"
$efiDir = Join-Path $repoRoot "assets\efi"
$manifestPath = Join-Path $efiDir "checksums.txt"
$grubPath = Join-Path $efiDir "grubx64.efi"
$bootPath = Join-Path $efiDir "bootx64.efi"
$provenancePath = Join-Path $repoRoot "docs\release-efi-provenance.md"
$standaloneScript = Join-Path $repoRoot "scripts\build-standalone-grub.ps1"

if (!(Test-Path $cargoToml)) { throw "Cargo.toml not found at $cargoToml" }
if (!(Test-Path $standaloneScript)) { throw "Missing $standaloneScript" }

if (-not $SkipProvenanceCheck) {
    Assert-ProvenanceComplete $provenancePath
    Write-Host "[ok] provenance check passed"
}

if (-not $SkipStandaloneGrubBuild) {
    Write-Host "[step] build standalone grubx64.efi"
    & $standaloneScript -OutputPath $grubPath
    if ($LASTEXITCODE -ne 0) {
        throw "Standalone GRUB build failed (exit code $LASTEXITCODE)"
    }
}

if (!(Test-Path $grubPath)) { throw "Missing $grubPath" }
if (!(Test-Path $bootPath)) { throw "Missing $bootPath" }
Assert-EfiSignatureMetadata $grubPath $false
Assert-EfiSignatureMetadata $bootPath $true
Write-Host "[ok] EFI signature metadata check passed"

$versionLine = Get-Content $cargoToml | Where-Object { $_ -match '^version\s*=' } | Select-Object -First 1
if (-not $versionLine) { throw "Could not find version in Cargo.toml" }
$version = ($versionLine -split "=", 2)[1].Trim().Trim('"')

$grubCrc = Get-Crc32Hex $grubPath
$bootCrc = Get-Crc32Hex $bootPath

if ($RefreshChecksums -or !(Test-Path $manifestPath)) {
    @(
        "grubx64.efi=$grubCrc"
        "bootx64.efi=$bootCrc"
    ) | Set-Content -Path $manifestPath -Encoding ascii
    Write-Host "[ok] wrote $manifestPath"
}

$checksums = Get-ChecksumsMap $manifestPath
if (-not $checksums.ContainsKey("grubx64.efi")) { throw "checksums.txt missing grubx64.efi entry" }
if (-not $checksums.ContainsKey("bootx64.efi")) { throw "checksums.txt missing bootx64.efi entry" }
if ($checksums["grubx64.efi"] -ne $grubCrc) {
    throw "Checksum mismatch for grubx64.efi (manifest=$($checksums["grubx64.efi"]) actual=$grubCrc)"
}
if ($checksums["bootx64.efi"] -ne $bootCrc) {
    throw "Checksum mismatch for bootx64.efi (manifest=$($checksums["bootx64.efi"]) actual=$bootCrc)"
}
Write-Host "[ok] verified bundled EFI checksums"

Push-Location $repoRoot
try {
    $buildCmd = "cargo +stable-$Target build --release --target $Target"
    Write-Host "[step] $buildCmd"
    Invoke-Expression $buildCmd
} finally {
    Pop-Location
}

$exePath = Join-Path $repoRoot "target\$Target\release\partboot.exe"
if (!(Test-Path $exePath)) {
    throw "Built binary not found at $exePath"
}

$outRootAbs = Join-Path $repoRoot $OutputRoot
$bundleName = "partboot-$version-$Target"
$bundleDir = Join-Path $outRootAbs $bundleName
$assetsOut = Join-Path $bundleDir "assets\efi"
$zipPath = Join-Path $outRootAbs "$bundleName.zip"

if (Test-Path $bundleDir) { Remove-Item $bundleDir -Recurse -Force }
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }

New-Item -ItemType Directory -Path $assetsOut -Force | Out-Null
Copy-Item $exePath (Join-Path $bundleDir "partboot.exe") -Force
Copy-Item (Join-Path $repoRoot "README.md") (Join-Path $bundleDir "README.md") -Force
Copy-Item $grubPath (Join-Path $assetsOut "grubx64.efi") -Force
Copy-Item $bootPath (Join-Path $assetsOut "bootx64.efi") -Force
Copy-Item $manifestPath (Join-Path $assetsOut "checksums.txt") -Force
if (Test-Path (Join-Path $efiDir "README.txt")) {
    Copy-Item (Join-Path $efiDir "README.txt") (Join-Path $assetsOut "README.txt") -Force
}
if (Test-Path (Join-Path $repoRoot "docs\release-efi-provenance.md")) {
    Copy-Item (Join-Path $repoRoot "docs\release-efi-provenance.md") (Join-Path $bundleDir "EFI-PROVENANCE.md") -Force
}

Compress-Archive -Path (Join-Path $bundleDir "*") -DestinationPath $zipPath -CompressionLevel Optimal

Write-Host "[ok] release bundle created: $bundleDir"
Write-Host "[ok] release zip created:    $zipPath"
