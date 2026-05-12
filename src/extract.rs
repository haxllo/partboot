use crate::cache;
use crate::iso::{IsoFamily, IsoImage};
use crate::layout::PartBootLayout;
use crate::spinner::Spinner;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXTRACTED_FILES_REQUIRED: [&str; 2] = ["vmlinuz", "initrd"];

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

const ROOTFS_CANDIDATES: [&str; 9] = [
    "casper/filesystem.squashfs",
    "live/filesystem.squashfs",
    "filesystem.squashfs",
    "casper/squashfs",
    "live/filesystem.squash",
    "casper/filesystem.squash",
    "arch/airootfs.sfs",
    "LiveOS/squashfs.img",
    "images/install.img",
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
        if !is_supported_linux_family(&image.family) {
            continue;
        }

        let extracted_id = extracted_id_from_iso_name(&image.name);
        if is_complete_extracted(layout, &extracted_id) {
            image.mark_extracted(extracted_id);
        }
    }
}

pub fn is_supported_linux_family(family: &IsoFamily) -> bool {
    matches!(
        family,
        IsoFamily::UbuntuCasper | IsoFamily::DebianLive | IsoFamily::Arch | IsoFamily::Fedora
    )
}

pub fn is_complete_extracted(layout: &PartBootLayout, extracted_id: &str) -> bool {
    let casper = casper_dir(layout, extracted_id);
    EXTRACTED_FILES_REQUIRED
        .iter()
        .all(|file| casper.join(file).exists())
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

// Try listing ISO contents using 7z -slt and return a list of entry paths (Path = ...)
fn run_7z_list_with(program: &str, iso_path: &Path) -> Result<Vec<String>, SevenZipError> {
    let output = Command::new(program)
        .arg("l")
        .arg("-slt")
        .arg(iso_path)
        .output()
        .map_err(|error| SevenZipError::Spawn(format!("{} ({})", program, error.to_string().trim())))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stderr.is_empty() { stdout } else { stderr };
        return Err(SevenZipError::Run(format!("7z failed listing ISO: {message}")));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Path = ") {
            entries.push(rest.trim().to_string());
        }
    }
    Ok(entries)
}

fn list_iso_entries(iso_path: &Path) -> Result<Vec<String>, String> {
    let mut spawn_errors = Vec::new();
    for program in seven_zip_candidates() {
        match run_7z_list_with(&program, iso_path) {
            Ok(list) => return Ok(list),
            Err(SevenZipError::Spawn(err)) => spawn_errors.push(err),
            Err(SevenZipError::Run(err)) => return Err(err),
        }
    }
    Err(format!("failed to run 7z list. Attempts: {}", spawn_errors.join("; ")))
}

fn score_match(path: &str, candidate: &str) -> i32 {
    let lower = path.to_lowercase();
    let cand = candidate.to_lowercase();
    let mut score = 0;
    if lower.ends_with(&cand) {
        score += 50;
    }
    if lower.contains("/casper/") || lower.contains("\\casper\\") || lower.contains("casper/") {
        score += 20;
    }
    if lower.contains("/live/") || lower.contains("live/") {
        score += 10;
    }
    if lower.contains("/install/") || lower.contains("install/") {
        score += 5;
    }
    // shorter paths slightly preferred
    score -= (lower.len() / 100) as i32;
    score
}

fn find_best_entry(entries: &[String], candidates: &[&str]) -> Option<String> {
    let mut best: Option<(i32, String)> = None;
    for entry in entries {
        for cand in candidates {
            let cand_basename = Path::new(cand)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(cand);
            if entry.to_lowercase().ends_with(&cand_basename.to_lowercase()) || entry.to_lowercase().contains(&cand_basename.to_lowercase()) {
                let sc = score_match(entry, cand_basename);
                if best.as_ref().map(|(s, _)| sc > *s).unwrap_or(true) {
                    best = Some((sc, entry.clone()));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

fn dynamic_scan_and_extract(iso_path: &Path, destination: &Path) -> Result<bool, String> {
    let entries = list_iso_entries(iso_path)?;

    let v_basenames = ["vmlinuz", "kernel"];
    let i_basenames = ["initrd.img", "initrd.lz", "initrd.gz", "initrd", "initramfs"];
    let s_basenames = [
        "filesystem.squashfs",
        "filesystem.squash",
        "squashfs",
        "airootfs.sfs",
        "install.img",
    ];

    let mut all_ok = true;

    // helper closure to extract and rename
    let do_extract = |best_path: Option<String>, canonical: &str| -> Result<bool, String> {
        if let Some(p) = best_path {
            // attempt extraction using existing run_7z_extract (it will try multiple programs)
            match run_7z_extract(iso_path, &p, destination) {
                Ok(()) => {
                    if let Some(basename) = Path::new(&p).file_name().and_then(|s| s.to_str()) {
                        let extracted_file = destination.join(basename);
                        if extracted_file.exists() {
                            fs::rename(&extracted_file, destination.join(canonical))
                                .map_err(|e| e.to_string())?;
                            return Ok(true);
                        }
                        // try to find any file that ends with basename
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
                        // maybe 7z extracted with the exact canonical name already
                        if destination.join(canonical).exists() {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
                Err(_) => Ok(false),
            }
        } else {
            Ok(false)
        }
    };

    let v_best = find_best_entry(&entries, &v_basenames);
    let v_ok = do_extract(v_best, "vmlinuz")?;
    if !v_ok { all_ok = false; }

    let i_best = find_best_entry(&entries, &i_basenames);
    let i_ok = do_extract(i_best, "initrd")?;
    if !i_ok { all_ok = false; }

    let s_best = find_best_entry(&entries, &s_basenames);
    let _ = do_extract(s_best, "filesystem.squashfs")?;

    Ok(all_ok)
}

pub fn extract_casper(layout: &PartBootLayout, iso_arg: &str) -> Result<String, String> {
    let iso_path = resolve_iso_path(layout, iso_arg)?;
    let iso_name = iso_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid ISO path: {}", iso_path.display()))?;
    let extracted_id = extracted_id_from_iso_name(iso_name);
    let destination = casper_dir(layout, &extracted_id);

    // Check cache first - if we've already extracted this ISO, skip re-extraction
    if let Ok(Some(_cached)) = cache::load_from_cache(&iso_path) {
        if is_complete_extracted(layout, &extracted_id) {
            return Ok(extracted_id);
        }
    }

    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;

    let spinner = Spinner::new(&format!("Extracting {}...", iso_name));
    let mut missing = Vec::new();

    // Extract vmlinuz
    match try_extract_candidates(&iso_path, &VMLINUZ_CANDIDATES, &destination, "vmlinuz") {
        Ok(true) => {},
        Ok(false) => missing.push("vmlinuz"),
        Err(e) => {
            spinner.finish_error(&format!("extraction failed: {}", e));
            return Err(format!("Extraction error: {}\n• Check if ISO file is corrupted", e));
        }
    }

    // Extract initrd
    match try_extract_candidates(&iso_path, &INITRD_CANDIDATES, &destination, "initrd") {
        Ok(true) => {},
        Ok(false) => missing.push("initrd"),
        Err(e) => {
            spinner.finish_error(&format!("extraction failed: {}", e));
            return Err(format!("Extraction error: {}\n• Check if ISO file is corrupted", e));
        }
    }

    // Extract filesystem
    let _ = try_extract_candidates(&iso_path, &ROOTFS_CANDIDATES, &destination, "filesystem.squashfs")
        .map_err(|e| e.to_string())?;

    // If initial candidate extraction failed for any file, run dynamic scanner as a fallback.
    if !is_complete_extracted(layout, &extracted_id) {
        match dynamic_scan_and_extract(&iso_path, &destination) {
            Ok(true) => {
                spinner.finish(&format!("Extraction complete ({})", iso_name));
                let vmlinuz_path = destination.join("vmlinuz").exists().then(|| "vmlinuz".to_string());
                let initrd_path = destination.join("initrd").exists().then(|| "initrd".to_string());
                let rootfs_path = destination.join("filesystem.squashfs").exists().then(|| "filesystem.squashfs".to_string());
                let _ = cache::save_to_cache(&iso_path, vmlinuz_path, initrd_path, rootfs_path, "Linux".to_string());
                return Ok(extracted_id);
            }
            Ok(false) => {
                let hint = if missing.contains(&"vmlinuz") && missing.contains(&"initrd") {
                    "• ISO may not be a standard Live ISO\n• Check if this is an installer ISO (Anaconda, etc.)"
                } else if missing.contains(&"vmlinuz") {
                    "• kernel file not found in standard locations"
                } else if missing.contains(&"initrd") {
                    "• initramfs not found in standard locations"
                } else {
                    "• core extraction files missing"
                };
                spinner.finish_error(&format!("extraction incomplete for {}", iso_name));
                return Err(format!("Extraction failed for {}.\nMissing: {}\n\nTroubleshoot:\n{}", extracted_id, missing.join(", "), hint));
            }
            Err(e) => {
                spinner.finish_error(&format!("extraction failed: {}", e));
                return Err(format!("Extraction error: {}\n• Check if ISO file is corrupted", e));
            }
        }
    }

    if !is_complete_extracted(layout, &extracted_id) {
        let hint = if missing.contains(&"vmlinuz") && missing.contains(&"initrd") {
            "• ISO may not be a standard Live ISO\n• Check if this is an installer ISO (Anaconda, etc.)"
        } else if missing.contains(&"vmlinuz") {
            "• kernel file not found in standard locations"
        } else if missing.contains(&"initrd") {
            "• initramfs not found in standard locations"
        } else {
            "• core extraction files missing"
        };
        spinner.finish_error(&format!("extraction incomplete for {}", iso_name));
        return Err(format!("Extraction failed for {}.\nMissing: {}\n\nTroubleshoot:\n{}", extracted_id, missing.join(", "), hint));
    }

    spinner.finish(&format!("Extraction complete ({})", iso_name));

    // Save to cache after successful extraction
    let vmlinuz_path = destination.join("vmlinuz").exists().then(|| "vmlinuz".to_string());
    let initrd_path = destination.join("initrd").exists().then(|| "initrd".to_string());
    let rootfs_path = destination.join("filesystem.squashfs").exists().then(|| "filesystem.squashfs".to_string());

    let _ = cache::save_to_cache(&iso_path, vmlinuz_path, initrd_path, rootfs_path, "Linux".to_string());

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

    #[test]
    fn supported_linux_family_detection() {
        assert!(is_supported_linux_family(&IsoFamily::UbuntuCasper));
        assert!(is_supported_linux_family(&IsoFamily::DebianLive));
        assert!(is_supported_linux_family(&IsoFamily::Arch));
        assert!(is_supported_linux_family(&IsoFamily::Fedora));
        assert!(!is_supported_linux_family(&IsoFamily::Windows));
        assert!(!is_supported_linux_family(&IsoFamily::Unknown));
    }
}
