use crate::iso::{IsoFamily, IsoImage};
use crate::layout::PartBootLayout;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CASPER_FILES: [&str; 3] = ["vmlinuz", "initrd", "filesystem.squashfs"];

// More robust candidate lists for different distro ISO layouts.
const VMLINUZ_CANDIDATES: [&str; 7] = [
    "casper/vmlinuz",
    "live/vmlinuz",
    "install/vmlinuz",
    "vmlinuz",
    "boot/vmlinuz",
    "casper/kernel",
    "kernel/vmlinuz",
];

const INITRD_CANDIDATES: [&str; 8] = [
    "casper/initrd",
    "casper/initrd.img",
    "casper/initrd.lz",
    "casper/initrd.gz",
    "live/initrd.img",
    "install/initrd.lz",
    "initrd.img",
    "initrd",
];

const SQUASHFS_CANDIDATES: [&str; 6] = [
    "casper/filesystem.squashfs",
    "live/filesystem.squashfs",
    "filesystem.squashfs",
    "casper/squashfs",
    "live/filesystem.squash",
    "casper/filesystem.squash",
];

pub fn extracted_id_from_iso_name(iso_name: &str) -> String {
    let stem = iso_name
        .strip_suffix(".iso")
        .or_else(|| iso_name.strip_suffix(".ISO"))
        .unwrap_or(iso_name);

    stem.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn mark_extracted_images(layout: &PartBootLayout, images: &mut [IsoImage]) {
    for image in images {
        if image.family != IsoFamily::UbuntuCasper {
            continue;
        }

        let extracted_id = extracted_id_from_iso_name(&image.name);
        if is_complete_extracted_casper(layout, &extracted_id) {
            image.mark_extracted(extracted_id);
        }
    }
}

pub fn is_complete_extracted_casper(layout: &PartBootLayout, extracted_id: &str) -> bool {
    let casper = casper_dir(layout, extracted_id);
    CASPER_FILES.iter().all(|file| casper.join(file).exists())
}

fn try_extract_candidates(
    iso_path: &Path,
    candidates: &[&str],
    destination: &Path,
    canonical: &str,
) -> Result<bool, String> {
    for cand in candidates {
        match run_7z_extract(iso_path, cand, destination) {
            Ok(()) => {
                // basename of the candidate (what 7z will write)
                if let Some(basename) = Path::new(cand).file_name().and_then(|s| s.to_str()) {
                    let extracted_file = destination.join(basename);
                    if extracted_file.exists() {
                        fs::rename(&extracted_file, destination.join(canonical))
                            .map_err(|e| e.to_string())?;
                        return Ok(true);
                    }

                    // sometimes 7z extracts with slightly different names or strips path; try to find a file that ends with basename
                    if let Ok(entries) = fs::read_dir(destination) {
                        for entry in entries.flatten() {
                            if let Ok(name) = entry.file_name().into_string() {
                                if name.ends_with(basename) {
                                    fs::rename(entry.path(), destination.join(canonical))
                                        .map_err(|e| e.to_string())?;
                                    return Ok(true);
                                }
                            }
                        }
                    }

                    // fallback: if candidate was directory-like, try to look for known canonical file inside destination
                    if destination.join(canonical).exists() {
                        return Ok(true);
                    }
                } else {
                    // no basename - try to pick any new file in directory
                    if let Ok(mut entries) = fs::read_dir(destination) {
                        if let Some(Ok(entry)) = entries.next() {
                            fs::rename(entry.path(), destination.join(canonical))
                                .map_err(|e| e.to_string())?;
                            return Ok(true);
                        }
                    }
                }
            }
            Err(_) => {
                // try next candidate
            }
        }
    }
    Ok(false)
}

pub fn extract_casper(layout: &PartBootLayout, iso_arg: &str) -> Result<String, String> {
    let iso_path = resolve_iso_path(layout, iso_arg)?;
    let iso_name = iso_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid ISO path: {}", iso_path.display()))?;
    let extracted_id = extracted_id_from_iso_name(iso_name);
    let destination = casper_dir(layout, &extracted_id);
    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;

    let mut missing = Vec::new();

    let v_ok = try_extract_candidates(&iso_path, &VMLINUZ_CANDIDATES, &destination, "vmlinuz")
        .map_err(|e| e.to_string())?;
    if !v_ok {
        missing.push(format!("vmlinuz (tried: {})", VMLINUZ_CANDIDATES.join(", ")));
    }

    let i_ok = try_extract_candidates(&iso_path, &INITRD_CANDIDATES, &destination, "initrd")
        .map_err(|e| e.to_string())?;
    if !i_ok {
        missing.push(format!("initrd (tried: {})", INITRD_CANDIDATES.join(", ")));
    }

    let s_ok = try_extract_candidates(&iso_path, &SQUASHFS_CANDIDATES, &destination, "filesystem.squashfs")
        .map_err(|e| e.to_string())?;
    if !s_ok {
        missing.push(format!("filesystem.squashfs (tried: {})", SQUASHFS_CANDIDATES.join(", ")));
    }

    if !is_complete_extracted_casper(layout, &extracted_id) {
        return Err(format!(
            "extraction incomplete for {}; missing: {}",
            extracted_id,
            missing.join("; ")
        ));
    }
    Ok(extracted_id)
}

fn resolve_iso_path(layout: &PartBootLayout, iso_arg: &str) -> Result<PathBuf, String> {
    let arg_path = PathBuf::from(iso_arg);
    if arg_path.is_absolute() {
        if arg_path.exists() {
            return Ok(arg_path);
        }
        return Err(format!("ISO not found: {}", arg_path.display()));
    }

    let path = layout.isos.join(iso_arg);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("ISO not found: {}", path.display()))
    }
}

fn casper_dir(layout: &PartBootLayout, extracted_id: &str) -> PathBuf {
    layout.extracted.join(extracted_id).join("casper")
}

fn run_7z_extract(iso_path: &Path, file_in_iso: &str, destination: &Path) -> Result<(), String> {
    let mut spawn_errors = Vec::new();
    for program in seven_zip_candidates() {
        match run_7z_extract_with(&program, iso_path, file_in_iso, destination) {
            Ok(()) => return Ok(()),
            Err(SevenZipError::Spawn(error)) => spawn_errors.push(error),
            Err(SevenZipError::Run(error)) => return Err(error),
        }
    }

    Err(format!(
        "failed to run 7z. Ensure 7-Zip is installed and available in PATH, or set PARTBOOT_7Z_PATH. Attempts: {}",
        spawn_errors.join("; ")
    ))
}

fn seven_zip_candidates() -> Vec<String> {
    let configured = std::env::var("PARTBOOT_7Z_PATH")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    seven_zip_candidates_with(configured.as_deref())
}

fn seven_zip_candidates_with(configured: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(value) = configured {
        candidates.push(value.to_string());
    }
    candidates.push("7z".to_string());
    candidates.push("7za".to_string());
    candidates
}

enum SevenZipError {
    Spawn(String),
    Run(String),
}

fn run_7z_extract_with(
    program: &str,
    iso_path: &Path,
    file_in_iso: &str,
    destination: &Path,
) -> Result<(), SevenZipError> {
    let output = Command::new(program)
        .arg("e")
        .arg(iso_path)
        .arg(file_in_iso)
        .arg(format!("-o{}", destination.display()))
        .arg("-y")
        .output()
        .map_err(|error| {
            SevenZipError::Spawn(format!("{} ({})", program, error.to_string().trim()))
        })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if stderr.is_empty() { stdout } else { stderr };
    Err(SevenZipError::Run(format!(
        "7z failed extracting {file_in_iso}: {message}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracted_id_strips_iso_extension_and_sanitizes() {
        assert_eq!(
            extracted_id_from_iso_name("ubuntu-22.04.5-desktop-amd64.iso"),
            "ubuntu-22.04.5-desktop-amd64"
        );
        assert_eq!(extracted_id_from_iso_name("Ubuntu Live.iso"), "Ubuntu_Live");
    }

    #[test]
    fn seven_zip_candidates_include_configured_path_first() {
        let candidates = seven_zip_candidates_with(Some("D:/tools/7z.exe"));
        assert_eq!(candidates[0], "D:/tools/7z.exe");
        assert_eq!(candidates[1], "7z");
        assert_eq!(candidates[2], "7za");
    }
}
