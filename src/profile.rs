use crate::extract::extracted_id_from_iso_name;
use crate::iso::{classify_name, IsoFamily, IsoImage};
use crate::layout::PartBootLayout;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    Extracted,
    IsoToram,
}

impl BootMode {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "extracted" => Some(BootMode::Extracted),
            "iso_toram" => Some(BootMode::IsoToram),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoProfile {
    pub name: String,
    pub family: IsoFamily,
    pub preferred_mode: BootMode,
    pub fallback_mode: BootMode,
    pub visible_fallback: bool,
}

pub fn ensure_profiles_for_images(
    layout: &PartBootLayout,
    images: &[IsoImage],
) -> Result<Vec<PathBuf>, String> {
    let mut created = Vec::new();
    fs::create_dir_all(&layout.profiles).map_err(|error| error.to_string())?;
    for image in images {
        if let Some(path) = ensure_profile_for_iso_name(layout, &image.name)? {
            created.push(path);
        }
    }
    Ok(created)
}

pub fn ensure_profile_for_iso_name(
    layout: &PartBootLayout,
    iso_name: &str,
) -> Result<Option<PathBuf>, String> {
    let lower = iso_name.to_ascii_lowercase();
    let family = classify_name(&lower);
    if !is_supported_profile_family(&family) {
        return Ok(None);
    }

    fs::create_dir_all(&layout.profiles).map_err(|error| error.to_string())?;
    let path = profile_path(layout, iso_name);
    let content = default_profile(iso_name, &family);
    if path.exists() {
        let current = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        if current == content {
            return Ok(None);
        }
        fs::write(&path, content).map_err(|error| error.to_string())?;
        return Ok(Some(path));
    }
    fs::write(&path, content).map_err(|error| error.to_string())?;
    Ok(Some(path))
}

pub fn load_profiles_for_images(
    layout: &PartBootLayout,
    images: &[IsoImage],
) -> Result<Vec<IsoProfile>, String> {
    let mut profiles = Vec::new();
    for image in images {
        if !is_supported_profile_family(&image.family) {
            continue;
        }
        let path = profile_path(layout, &image.name);
        if !path.exists() {
            continue;
        }
        let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        profiles.push(parse_profile(&raw, &path)?);
    }
    Ok(profiles)
}

fn profile_path(layout: &PartBootLayout, iso_name: &str) -> PathBuf {
    let stem = extracted_id_from_iso_name(iso_name);
    layout.profiles.join(format!("{stem}.profile"))
}

fn default_profile(iso_name: &str, family: &IsoFamily) -> String {
    let family_label = profile_family_name(family);
    format!(
        "name={}\nfamily={}\npreferred_mode=iso_toram\nfallback_mode=iso_toram\nvisible_fallback=false\n",
        iso_name, family_label
    )
}

fn is_supported_profile_family(family: &IsoFamily) -> bool {
    matches!(
        family,
        IsoFamily::UbuntuCasper | IsoFamily::DebianLive | IsoFamily::Arch | IsoFamily::Fedora
    )
}

fn profile_family_name(family: &IsoFamily) -> &'static str {
    match family {
        IsoFamily::UbuntuCasper => "ubuntu",
        IsoFamily::DebianLive => "debian",
        IsoFamily::Arch => "arch",
        IsoFamily::Fedora => "fedora",
        IsoFamily::Windows | IsoFamily::Unknown => "unknown",
    }
}

fn parse_profile(content: &str, path: &PathBuf) -> Result<IsoProfile, String> {
    let mut values = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed
            .split_once('=')
            .ok_or_else(|| format!("invalid profile line in {}: {trimmed}", path.display()))?;
        values.insert(key.trim().to_string(), value.trim().to_string());
    }

    let name = required_value(&values, "name", path)?;
    let family = match required_value(&values, "family", path)?.as_str() {
        "ubuntu" => IsoFamily::UbuntuCasper,
        "debian" => IsoFamily::DebianLive,
        "arch" => IsoFamily::Arch,
        "fedora" => IsoFamily::Fedora,
        value => {
            return Err(format!(
                "unsupported profile family '{value}' in {}",
                path.display()
            ));
        }
    };
    let preferred_mode = BootMode::parse(&required_value(&values, "preferred_mode", path)?)
        .ok_or_else(|| format!("invalid preferred_mode in {}", path.display()))?;
    let fallback_mode = BootMode::parse(&required_value(&values, "fallback_mode", path)?)
        .ok_or_else(|| format!("invalid fallback_mode in {}", path.display()))?;
    let visible_fallback = match required_value(&values, "visible_fallback", path)?.as_str() {
        "true" => true,
        "false" => false,
        _ => {
            return Err(format!(
                "visible_fallback must be true/false in {}",
                path.display()
            ));
        }
    };

    Ok(IsoProfile {
        name,
        family,
        preferred_mode,
        fallback_mode,
        visible_fallback,
    })
}

fn required_value(
    values: &HashMap<String, String>,
    key: &str,
    path: &PathBuf,
) -> Result<String, String> {
    values
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing {key} in {}", path.display()))
}

pub fn count_profile_files(layout: &PartBootLayout) -> Result<usize, String> {
    if !layout.profiles.exists() {
        return Ok(0);
    }
    let mut count = 0usize;
    for entry in fs::read_dir(&layout.profiles).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("profile"))
            .unwrap_or(false)
        {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_layout_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("partboot-profile-test-{unique}"))
    }

    #[test]
    fn ensure_profile_rewrites_stale_content() {
        let root = temp_layout_root();
        let layout = PartBootLayout::new(&root);
        fs::create_dir_all(&layout.profiles).unwrap();

        let profile_path = layout.profiles.join("ubuntu-24.04.profile");
        fs::write(&profile_path, "name=ubuntu-24.04.iso\nfamily=ubuntu\npreferred_mode=extracted\nfallback_mode=iso_toram\nvisible_fallback=true\n")
            .unwrap();

        let changed = ensure_profile_for_iso_name(&layout, "ubuntu-24.04.iso")
            .expect("profile repair should succeed");
        assert_eq!(changed, Some(profile_path.clone()));

        let repaired = fs::read_to_string(&profile_path).unwrap();
        assert!(repaired.contains("preferred_mode=iso_toram"));
        assert!(repaired.contains("visible_fallback=false"));

        let unchanged = ensure_profile_for_iso_name(&layout, "ubuntu-24.04.iso")
            .expect("second repair should succeed");
        assert_eq!(unchanged, None);

        let _ = fs::remove_dir_all(&root);
    }
}
