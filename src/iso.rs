use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsoFamily {
    UbuntuCasper,
    DebianLive,
    Arch,
    Fedora,
    Windows,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportLevel {
    Supported,
    Experimental,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoImage {
    pub name: String,
    pub path: PathBuf,
    pub family: IsoFamily,
    pub support: SupportLevel,
    pub extracted_id: Option<String>,
}

impl IsoImage {
    pub fn from_path(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown.iso")
            .to_string();
        let lower = name.to_ascii_lowercase();
        let family = classify_name(&lower);
        let support = match family {
            IsoFamily::UbuntuCasper
            | IsoFamily::DebianLive
            | IsoFamily::Arch
            | IsoFamily::Fedora => SupportLevel::Supported,
            IsoFamily::Windows => SupportLevel::Experimental,
            IsoFamily::Unknown => SupportLevel::Unsupported,
        };

        Self {
            name,
            path,
            family,
            support,
            extracted_id: None,
        }
    }

    pub fn mark_extracted(&mut self, extracted_id: String) {
        self.extracted_id = Some(extracted_id);
    }
}

pub fn scan_iso_dir(dir: &Path) -> io::Result<Vec<IsoImage>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut images = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("iso"))
            .unwrap_or(false)
        {
            images.push(IsoImage::from_path(path));
        }
    }
    images.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    Ok(images)
}

pub fn classify_name(lower_name: &str) -> IsoFamily {
    if lower_name.contains("ubuntu")
        || lower_name.contains("linuxmint")
        || lower_name.contains("pop-os")
        || lower_name.contains("elementary")
    {
        IsoFamily::UbuntuCasper
    } else if lower_name.contains("debian") || lower_name.contains("kali") {
        IsoFamily::DebianLive
    } else if lower_name.contains("archlinux")
        || lower_name.contains("endeavouros")
        || lower_name.contains("omarchy")
        || lower_name.contains("cachyos")
    {
        IsoFamily::Arch
    } else if lower_name.contains("fedora") {
        IsoFamily::Fedora
    } else if lower_name.contains("win") || lower_name.contains("windows") {
        IsoFamily::Windows
    } else {
        IsoFamily::Unknown
    }
}

pub fn support_label(level: &SupportLevel) -> &'static str {
    match level {
        SupportLevel::Supported => "supported",
        SupportLevel::Experimental => "experimental",
        SupportLevel::Unsupported => "unsupported",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_known_iso_names() {
        assert_eq!(classify_name("ubuntu-24.04.iso"), IsoFamily::UbuntuCasper);
        assert_eq!(classify_name("archlinux-2026.iso"), IsoFamily::Arch);
        assert_eq!(classify_name("omarchy-3.2.3-2.iso"), IsoFamily::Arch);
        assert_eq!(classify_name("cachyos-desktop.iso"), IsoFamily::Arch);
        assert_eq!(classify_name("fedora-workstation.iso"), IsoFamily::Fedora);
        assert_eq!(classify_name("win11.iso"), IsoFamily::Windows);
        assert_eq!(classify_name("toolbox.iso"), IsoFamily::Unknown);
    }
}
