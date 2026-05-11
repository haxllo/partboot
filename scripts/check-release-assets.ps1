param(
    [string]$Tag = "",
    [string]$Repo = "haxllo/partboot"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Tag)) {
    throw "Tag is required. Example: .\scripts\check-release-assets.ps1 -Tag v0.1.3"
}

$requiredAssets = @(
    "partboot.exe",
    "bootx64.efi",
    "grubx64.efi",
    "checksums.txt",
    "partboot-$($Tag.TrimStart('v'))-x86_64-pc-windows-gnu.zip"
)

$releaseJson = gh release view $Tag --repo $Repo --json assets
if ([string]::IsNullOrWhiteSpace($releaseJson)) {
    throw "Failed to retrieve release metadata for $Repo $Tag"
}

$release = $releaseJson | ConvertFrom-Json
$assetNames = @($release.assets | ForEach-Object { $_.name })

$missing = @()
foreach ($asset in $requiredAssets) {
    if ($assetNames -notcontains $asset) {
        $missing += $asset
    }
}

if ($missing.Count -gt 0) {
    Write-Host "[error] Missing release assets for ${Tag}:" -ForegroundColor Red
    foreach ($name in $missing) {
        Write-Host " - $name" -ForegroundColor Red
    }
    throw "Release asset guard failed for $Tag"
}

Write-Host "[ok] Release asset guard passed for $Tag"
