param(
    [string]$OutputPath = "assets\efi\grubx64.efi",
    [string]$BootstrapConfigPath = "",
    [string]$Target = "x86_64-efi",
    [string]$Modules = "normal configfile search search_fs_uuid part_gpt part_msdos fat ntfs chain linux echo"
)

$ErrorActionPreference = "Stop"

function Resolve-GrubMkStandalone() {
    if ($env:PARTBOOT_GRUB_MKSTANDALONE -and -not [string]::IsNullOrWhiteSpace($env:PARTBOOT_GRUB_MKSTANDALONE)) {
        if (Test-Path $env:PARTBOOT_GRUB_MKSTANDALONE) {
            return (Resolve-Path $env:PARTBOOT_GRUB_MKSTANDALONE).Path
        }
        throw "PARTBOOT_GRUB_MKSTANDALONE is set but not found: $($env:PARTBOOT_GRUB_MKSTANDALONE)"
    }

    $command = Get-Command grub-mkstandalone -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    throw "grub-mkstandalone not found. Install GRUB tools or set PARTBOOT_GRUB_MKSTANDALONE."
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$outputAbs = if ([System.IO.Path]::IsPathRooted($OutputPath)) { $OutputPath } else { Join-Path $repoRoot $OutputPath }
$outputDir = Split-Path -Parent $outputAbs
if (-not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

$bootstrapAbs = $null
if ($BootstrapConfigPath -and -not [string]::IsNullOrWhiteSpace($BootstrapConfigPath)) {
    $bootstrapAbs = if ([System.IO.Path]::IsPathRooted($BootstrapConfigPath)) { $BootstrapConfigPath } else { Join-Path $repoRoot $BootstrapConfigPath }
    if (-not (Test-Path $bootstrapAbs)) {
        throw "Bootstrap config not found: $bootstrapAbs"
    }
} else {
    $bootstrapAbs = Join-Path $env:TEMP "partboot-grub-bootstrap.cfg"
    @(
        "set pager=1"
        "set prefix=(`$cmdpath)"
        "if [ -f (`$cmdpath)/grub.cfg ]; then"
        "  configfile (`$cmdpath)/grub.cfg"
        "fi"
        "echo 'PartBoot: grub.cfg not found next to grubx64.efi'"
        "echo 'Expected: (`$cmdpath)/grub.cfg'"
    ) | Set-Content -Path $bootstrapAbs -Encoding ascii
}

$grubMkStandalone = Resolve-GrubMkStandalone
$bootstrapMap = "boot/grub/grub.cfg=$bootstrapAbs"

Write-Host "[step] building standalone GRUB EFI"
Write-Host "[info] tool: $grubMkStandalone"
Write-Host "[info] output: $outputAbs"
Write-Host "[info] bootstrap: $bootstrapAbs"

& $grubMkStandalone -O $Target -o $outputAbs --modules $Modules $bootstrapMap

if ($LASTEXITCODE -ne 0) {
    throw "grub-mkstandalone failed with exit code $LASTEXITCODE"
}
if (-not (Test-Path $outputAbs)) {
    throw "Standalone GRUB output not found: $outputAbs"
}

Write-Host "[ok] built standalone GRUB EFI: $outputAbs"
