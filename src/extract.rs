use crate::iso::{IsoFamily, IsoImage};
use crate::layout::PartBootLayout;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CASPER_FILES: [&str; 3] = ["vmlinuz", "initrd", "filesystem.squashfs"];

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

pub fn extract_casper(layout: &PartBootLayout, iso_arg: &str) -> Result<String, String> {
    let iso_path = resolve_iso_path(layout, iso_arg)?;
    let iso_name = iso_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid ISO path: {}", iso_path.display()))?;
    let extracted_id = extracted_id_from_iso_name(iso_name);
    let destination = casper_dir(layout, &extracted_id);
    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;

    for file in CASPER_FILES {
        run_7z_extract(&iso_path, &format!("casper\\{file}"), &destination)?;
    }

    if !is_complete_extracted_casper(layout, &extracted_id) {
        return Err(format!(
            "extraction incomplete for {}; expected vmlinuz, initrd, and filesystem.squashfs",
            extracted_id
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
