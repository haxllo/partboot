use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareBootEntry {
    pub kind: String,
    pub identifier: String,
    pub description: Option<String>,
    pub device: Option<String>,
    pub path: Option<String>,
    pub display_order_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateBootEntryResult {
    pub identifier: String,
    pub label: String,
    pub loader: String,
    pub backup_path: Option<PathBuf>,
    pub added_first: bool,
    pub reused_existing: bool,
    pub dry_run: bool,
    pub secure_boot_enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveBootEntryResult {
    pub identifier: String,
    pub backup_path: Option<PathBuf>,
    pub dry_run: bool,
    pub secure_boot_enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreBootEntryResult {
    pub backup_path: PathBuf,
    pub dry_run: bool,
    pub secure_boot_enabled: Option<bool>,
}

pub fn list_firmware_entries(partboot_only: bool) -> Result<Vec<FirmwareBootEntry>, String> {
    #[cfg(not(windows))]
    {
        Err("boot-entry is currently supported on Windows UEFI only".to_string())
    }

    #[cfg(windows)]
    {
        ensure_windows_uefi()?;
        let output = run_bcdedit(&["/enum", "firmware", "/v"])?;
        let mut entries = parse_firmware_entries(&output);
        if partboot_only {
            entries.retain(|entry| {
                entry
                    .path
                    .as_deref()
                    .map(|path| path.to_ascii_lowercase().contains("\\efi\\partboot\\"))
                    .unwrap_or(false)
            });
        }
        Ok(entries)
    }
}

pub fn create_boot_entry(
    esp: &Path,
    root: Option<&Path>,
    label: &str,
    loader: Option<&str>,
    add_first: bool,
    dry_run: bool,
) -> Result<CreateBootEntryResult, String> {
    #[cfg(not(windows))]
    {
        let _ = (esp, root, label, loader, add_first, dry_run);
        Err("boot-entry is currently supported on Windows UEFI only".to_string())
    }

    #[cfg(windows)]
    {
        ensure_windows_uefi()?;
        validate_esp_path(esp)?;
        let loader = resolve_loader_path(root, loader)?;
        let loader_full_path = resolve_loader_full_path(esp, &loader);
        if !loader_full_path.exists() {
            return Err(format!(
                "loader file does not exist: {}",
                loader_full_path.display()
            ));
        }

        if dry_run {
            return Ok(CreateBootEntryResult {
                identifier: "{dry-run}".to_string(),
                label: label.to_string(),
                loader,
                backup_path: None,
                added_first: add_first,
                reused_existing: false,
                dry_run: true,
                secure_boot_enabled: secure_boot_state(),
            });
        }

        ensure_admin()?;
        let backup_path = export_bcd_backup()?;

        let existing = find_existing_entry(label, &loader)?;
        let reused_existing = existing.is_some();
        let identifier = if let Some(existing_id) = existing {
            existing_id
        } else {
            let copy_output = run_bcdedit(&["/copy", "{bootmgr}", "/d", label])?;
            parse_copied_identifier(&copy_output).ok_or_else(|| {
                format!(
                    "failed to parse new boot-entry GUID from bcdedit output: {}",
                    copy_output.trim()
                )
            })?
        };

        let drive = drive_from_esp(esp)?;
        let device_value = format!("partition={drive}");
        run_bcdedit(&["/set", &identifier, "device", &device_value])?;
        run_bcdedit(&["/set", &identifier, "path", &loader])?;
        if add_first {
            run_bcdedit(&["/set", "{fwbootmgr}", "displayorder", &identifier, "/addfirst"])?;
        } else {
            run_bcdedit(&["/set", "{fwbootmgr}", "displayorder", &identifier, "/addlast"])?;
        }

        Ok(CreateBootEntryResult {
            identifier,
            label: label.to_string(),
            loader,
            backup_path: Some(backup_path),
            added_first: add_first,
            reused_existing,
            dry_run: false,
            secure_boot_enabled: secure_boot_state(),
        })
    }
}

pub fn remove_boot_entry(identifier: &str, dry_run: bool) -> Result<RemoveBootEntryResult, String> {
    #[cfg(not(windows))]
    {
        let _ = (identifier, dry_run);
        Err("boot-entry is currently supported on Windows UEFI only".to_string())
    }

    #[cfg(windows)]
    {
        ensure_windows_uefi()?;
        validate_removable_identifier(identifier)?;
        let secure_boot = secure_boot_state();

        if dry_run {
            return Ok(RemoveBootEntryResult {
                identifier: identifier.to_string(),
                backup_path: None,
                dry_run: true,
                secure_boot_enabled: secure_boot,
            });
        }

        ensure_admin()?;
        let backup_path = export_bcd_backup()?;
        run_bcdedit(&["/delete", identifier, "/cleanup"])?;

        Ok(RemoveBootEntryResult {
            identifier: identifier.to_string(),
            backup_path: Some(backup_path),
            dry_run: false,
            secure_boot_enabled: secure_boot,
        })
    }
}

pub fn restore_boot_entries(backup_path: &Path, dry_run: bool) -> Result<RestoreBootEntryResult, String> {
    #[cfg(not(windows))]
    {
        let _ = (backup_path, dry_run);
        Err("boot-entry is currently supported on Windows UEFI only".to_string())
    }

    #[cfg(windows)]
    {
        ensure_windows_uefi()?;
        if !backup_path.exists() {
            return Err(format!("backup file does not exist: {}", backup_path.display()));
        }
        let secure_boot = secure_boot_state();
        if dry_run {
            return Ok(RestoreBootEntryResult {
                backup_path: backup_path.to_path_buf(),
                dry_run: true,
                secure_boot_enabled: secure_boot,
            });
        }
        ensure_admin()?;
        run_bcdedit(&["/import", &backup_path.to_string_lossy()])?;
        Ok(RestoreBootEntryResult {
            backup_path: backup_path.to_path_buf(),
            dry_run: false,
            secure_boot_enabled: secure_boot,
        })
    }
}

fn parse_firmware_entries(text: &str) -> Vec<FirmwareBootEntry> {
    let display_order = parse_fw_display_order(text);
    let mut entries = Vec::new();
    for block in text.split("\n\n") {
        let mut lines = block
            .lines()
            .map(|line| line.trim_end())
            .filter(|line| !line.trim().is_empty());
        let Some(header) = lines.next() else {
            continue;
        };

        let mut identifier = None;
        let mut description = None;
        let mut device = None;
        let mut path = None;
        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with('-') {
                continue;
            }
            if let Some(value) = value_after_key(trimmed, "identifier") {
                identifier = Some(value.to_string());
                continue;
            }
            if let Some(value) = value_after_key(trimmed, "description") {
                description = Some(value.to_string());
                continue;
            }
            if let Some(value) = value_after_key(trimmed, "device") {
                device = Some(value.to_string());
                continue;
            }
            if let Some(value) = value_after_key(trimmed, "path") {
                path = Some(value.to_string());
            }
        }

        let Some(identifier) = identifier else {
            continue;
        };
        if identifier.eq_ignore_ascii_case("{fwbootmgr}") {
            continue;
        }

        entries.push(FirmwareBootEntry {
            kind: header.trim().to_string(),
            identifier,
            description,
            device,
            path,
            display_order_index: None,
        });
    }
    for entry in &mut entries {
        if let Some(index) = display_order
            .iter()
            .position(|id| id.eq_ignore_ascii_case(&entry.identifier))
        {
            entry.display_order_index = Some(index);
        }
    }
    entries
}

fn parse_fw_display_order(text: &str) -> Vec<String> {
    for block in text.split("\n\n") {
        if !block.to_ascii_lowercase().contains("{fwbootmgr}") {
            continue;
        }
        let mut list = Vec::new();
        let lines: Vec<&str> = block.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if !trimmed.starts_with("displayorder") {
                continue;
            }
            list.extend(extract_braced_ids(trimmed));
            for cont in &lines[(idx + 1)..] {
                let value = cont.trim();
                if value.is_empty() || value.starts_with('-') {
                    break;
                }
                if value.contains(':') {
                    break;
                }
                list.extend(extract_braced_ids(value));
            }
            break;
        }
        return list;
    }
    Vec::new()
}

fn extract_braced_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut start_at = 0usize;
    while let Some(open_rel) = text[start_at..].find('{') {
        let open = start_at + open_rel;
        let Some(close_rel) = text[open..].find('}') else {
            break;
        };
        let close = open + close_rel;
        ids.push(text[open..=close].to_string());
        start_at = close + 1;
    }
    ids
}

fn value_after_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    if !line.starts_with(key) {
        return None;
    }
    let rest = line[key.len()..].trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

fn parse_copied_identifier(output: &str) -> Option<String> {
    let start = output.find('{')?;
    let end_rel = output[start..].find('}')?;
    let end = start + end_rel;
    let guid = &output[start..=end];
    if guid.len() >= 4 {
        Some(guid.to_string())
    } else {
        None
    }
}

#[cfg(windows)]
fn resolve_loader_path(root: Option<&Path>, loader: Option<&str>) -> Result<String, String> {
    if let Some(loader) = loader {
        return normalize_loader_path(loader);
    }
    if let Some(root) = root {
        let root = root.to_string_lossy().replace('\\', "/");
        if root.to_ascii_lowercase().ends_with("/partboot") {
            return Ok("\\EFI\\PartBoot\\grubx64.efi".to_string());
        }
        return Err("when using --root, root path must end with 'partboot'".to_string());
    }
    Err("create requires --loader, or --root to auto-resolve loader".to_string())
}

#[cfg(windows)]
fn find_existing_entry(label: &str, loader: &str) -> Result<Option<String>, String> {
    let entries = list_firmware_entries(false)?;
    for entry in entries {
        let desc = entry.description.as_deref().unwrap_or_default();
        let path = entry.path.as_deref().unwrap_or_default();
        if desc.eq_ignore_ascii_case(label) && path.eq_ignore_ascii_case(loader) {
            return Ok(Some(entry.identifier));
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn run_bcdedit(args: &[&str]) -> Result<String, String> {
    let output = Command::new("bcdedit")
        .args(args)
        .output()
        .map_err(|error| format!("failed to run bcdedit: {error}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!("bcdedit {} failed: {}", args.join(" "), details))
}

#[cfg(windows)]
fn ensure_windows_uefi() -> Result<(), String> {
    run_bcdedit(&["/enum", "{fwbootmgr}"]).map(|_| ())
}

#[cfg(windows)]
fn ensure_admin() -> Result<(), String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[bool]([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)",
        ])
        .output()
        .map_err(|error| format!("failed to run admin-check command: {error}"))?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_ascii_lowercase();
    if output.status.success() && value == "true" {
        Ok(())
    } else {
        Err("boot-entry create/remove requires an elevated shell (Run as Administrator)".to_string())
    }
}

#[cfg(windows)]
fn validate_esp_path(esp: &Path) -> Result<(), String> {
    if !esp.exists() {
        return Err(format!("ESP path does not exist: {}", esp.display()));
    }
    let drive = drive_from_esp(esp)?;
    if drive.len() != 2 || !drive.ends_with(':') {
        return Err(format!("invalid ESP drive format: {drive}"));
    }
    Ok(())
}

#[cfg(windows)]
fn normalize_loader_path(loader: &str) -> Result<String, String> {
    let trimmed = loader.trim();
    if trimmed.is_empty() {
        return Err("loader path cannot be empty".to_string());
    }
    if trimmed.contains(':') {
        return Err("loader must be an ESP-relative path like \\EFI\\PartBoot\\grubx64.efi".to_string());
    }
    let mut normalized = trimmed.replace('/', "\\");
    if !normalized.starts_with('\\') {
        normalized = format!("\\{normalized}");
    }
    Ok(normalized)
}

#[cfg(windows)]
fn resolve_loader_full_path(esp: &Path, loader: &str) -> PathBuf {
    let relative = loader.trim_start_matches('\\').replace('\\', "/");
    esp.join(relative)
}

#[cfg(windows)]
fn drive_from_esp(esp: &Path) -> Result<String, String> {
    let value = esp.to_string_lossy();
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        Ok(format!("{}:", (bytes[0] as char).to_ascii_uppercase()))
    } else {
        Err(format!(
            "ESP path must start with a drive letter, got {}",
            esp.display()
        ))
    }
}

#[cfg(windows)]
fn validate_removable_identifier(identifier: &str) -> Result<(), String> {
    let id = identifier.trim().to_ascii_lowercase();
    if !id.starts_with('{') || !id.ends_with('}') {
        return Err("identifier must look like {xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}".to_string());
    }
    let protected = ["{bootmgr}", "{fwbootmgr}", "{current}", "{default}", "{memdiag}"];
    if protected.iter().any(|value| *value == id) {
        return Err(format!("refusing to remove protected identifier {identifier}"));
    }
    Ok(())
}

#[cfg(windows)]
fn export_bcd_backup() -> Result<PathBuf, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    let backup = std::env::temp_dir().join(format!("partboot-bcd-backup-{unique}.bak"));
    run_bcdedit(&["/export", &backup.to_string_lossy()])?;
    Ok(backup)
}

#[cfg(windows)]
fn secure_boot_state() -> Option<bool> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", "Confirm-SecureBootUEFI"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_ascii_lowercase();
    match value.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_copied_identifier_extracts_guid() {
        let sample = "The entry was successfully copied to {12345678-1234-1234-1234-123456789ABC}.";
        assert_eq!(
            parse_copied_identifier(sample),
            Some("{12345678-1234-1234-1234-123456789ABC}".to_string())
        );
    }

    #[test]
    fn parse_firmware_entries_reads_basic_blocks() {
        let sample = "\
Firmware Boot Manager\n\
---------------------\n\
identifier              {fwbootmgr}\n\
displayorder            {bootmgr}\n\
\n\
Firmware Application (101fffff)\n\
-------------------------------\n\
identifier              {18c652b6-0073-11ed-bff6-806e6f6e6963}\n\
device                  partition=\\Device\\HarddiskVolume2\n\
path                    \\EFI\\UBUNTU\\SHIMX64.EFI\n\
description             Ubuntu\n";
        let entries = parse_firmware_entries(sample);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].identifier,
            "{18c652b6-0073-11ed-bff6-806e6f6e6963}"
        );
        assert_eq!(entries[0].description, Some("Ubuntu".to_string()));
    }

    #[test]
    fn parse_fw_display_order_extracts_ids() {
        let sample = "\
Firmware Boot Manager\n\
---------------------\n\
identifier              {fwbootmgr}\n\
displayorder            {bootmgr}\n\
                        {11111111-1111-1111-1111-111111111111}\n\
                        {22222222-2222-2222-2222-222222222222}\n\
";
        let ids = parse_fw_display_order(sample);
        assert_eq!(
            ids,
            vec![
                "{bootmgr}".to_string(),
                "{11111111-1111-1111-1111-111111111111}".to_string(),
                "{22222222-2222-2222-2222-222222222222}".to_string()
            ]
        );
    }
}
