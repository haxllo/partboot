# PartBoot

PartBoot is a disk-resident ISO boot manager. Boot Linux ISOs from an SSD/HDD partition instead of a USB flash drive.

## Getting Started

### Quick Start

Run the interactive wizard:

```
partboot start
```

This will:
1. Detect your available partitions
2. Auto-import ISO files from the selected drive
3. Extract boot files from supported Linux ISOs
4. Generate a GRUB boot menu
5. Show you how to install it to your EFI partition

### Supported ISOs

- Ubuntu (all variants)
- Debian / Kali Linux
- Arch Linux
- Fedora
- Most GRUB-compatible Linux distributions

Windows installer ISOs are detected but not yet supported.

## How It Works

PartBoot creates a `partboot` directory on your chosen partition:

```
H:\partboot
├─ isos\           (your ISO files)
├─ cache\          (downloaded EFI binaries)
├─ extracted\      (extracted boot files)
├─ profiles\       (boot configurations)
├─ efi\            (staged GRUB files)
└─ generated\      (final GRUB menu)
```

When you run `partboot start`, it:
1. **Imports ISOs** from your drive root if the directory is empty
2. **Extracts boot files** for supported Linux distributions
3. **Generates a GRUB menu** that boots any of them
4. **Auto-downloads EFI binaries** from GitHub if not bundled
5. **Shows installation steps** to copy files to your EFI partition

## Installation to EFI

Once you're happy with the generated boot menu, copy the files to your EFI partition:

```
partboot install-esp --root <PARTITION_PATH> --esp <EFI_PARTITION_PATH> --force
partboot boot-instructions --esp <EFI_PARTITION_PATH>
```

Then reboot and select the new boot entry from your firmware menu.

## Troubleshooting

## ISO Support

Supported in the generated GRUB menu:

- Ubuntu-style Casper live ISOs
- Debian/Kali-style live ISOs
- Arch-style live ISOs
- Fedora-style live ISOs

Experimental or blocked:

- Windows installer ISOs are detected, but the MVP emits a disabled menu entry.
  Windows support needs a `wimboot` or extracted-installer backend.
- Unknown ISOs are detected, but need explicit boot profiles.

## Testing Partition Recommendation

Yes, create a separate disposable partition for testing.

Start with **one NTFS partition** around 16-64 GB. That is the safest first
target from Windows because it supports large ISO files and is easy to inspect.
Use it only for PartBoot testing, for example as `H:\partboot`.

Add filesystems in this order:

1. **NTFS** first: best first test target on Windows; supports large ISOs.
2. **FAT32** later: useful for EFI-file experiments, but cannot store files over
   4 GB.
3. **ext4** later: useful for Linux-first testing, but Windows will not manage it
   comfortably.

Do not test on a partition that contains personal data, an installed OS, or a
recovery image.


## Advanced Usage

For advanced workflows (command-line scripting, JSON output, custom boot profiles), see [DEVELOPMENT.md](./DEVELOPMENT.md).

## Troubleshooting

### PartBoot fails to start

**Error: "Cannot find 7z"**
- Install 7-Zip from the Microsoft Store or https://www.7-zip.org
- Or set `PARTBOOT_7Z_PATH=C:\Program Files\7-Zip\7z.exe` in Environment Variables

**Error: "Cannot detect partition"**
- Your partition must be mounted and visible in File Explorer
- Try selecting a different drive letter in the partition menu
- Check that your drive supports NTFS or FAT32 (exFAT is not supported)

### Boot menu has no entries

- ISO files must be in the `partboot/isos/` directory
- Ubuntu ISOs must be "live" (desktop) variants, not server or minimal versions
- If ISO extraction fails, check that your partition has at least 2 GB free space

### Ubuntu boots but shows errors on shutdown

This is expected behavior when using the ISO boot mode. The system is tearing down a live session that still depends on the ISO file.

Workaround: After booting, save your files and shut down normally. Avoid force-resets.


## Testing Partition Recommendation

Yes, create a separate disposable partition for testing.

Start with **one NTFS partition** around 16-64 GB. That is the safest first
target from Windows because it supports large ISO files and is easy to inspect.
Use it only for PartBoot testing, for example as `H:\partboot`.

Add filesystems in this order:

1. **NTFS** first: best first test target on Windows; supports large ISOs.
2. **FAT32** later: useful for EFI-file experiments, but cannot store files over 4 GB.
3. **ext4** later: useful for Linux-first testing, but Windows will not manage it comfortably.

Do not test on a partition that contains personal data, an installed OS, or a recovery image.

## Advanced Usage

For advanced workflows (command-line scripting, JSON output, custom boot profiles), see [DEVELOPMENT.md](./DEVELOPMENT.md).
