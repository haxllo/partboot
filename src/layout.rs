use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartBootLayout {
    pub root: PathBuf,
    pub isos: PathBuf,
    pub profiles: PathBuf,
    pub cache: PathBuf,
    pub extracted: PathBuf,
    pub generated: PathBuf,
}

impl PartBootLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            isos: root.join("isos"),
            profiles: root.join("profiles"),
            cache: root.join("cache"),
            extracted: root.join("extracted"),
            generated: root.join("generated"),
            root,
        }
    }

    pub fn ensure(&self) -> io::Result<()> {
        fs::create_dir_all(&self.isos)?;
        fs::create_dir_all(&self.profiles)?;
        fs::create_dir_all(&self.cache)?;
        fs::create_dir_all(&self.extracted)?;
        fs::create_dir_all(&self.generated)?;
        Ok(())
    }

    pub fn grub_cfg_path(&self) -> PathBuf {
        self.generated.join("grub.cfg")
    }
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_paths_are_stable() {
        let layout = PartBootLayout::new("X:/partboot");
        assert_eq!(display_path(&layout.isos), "X:/partboot/isos");
        assert_eq!(display_path(&layout.profiles), "X:/partboot/profiles");
        assert_eq!(display_path(&layout.cache), "X:/partboot/cache");
        assert_eq!(display_path(&layout.extracted), "X:/partboot/extracted");
        assert_eq!(display_path(&layout.generated), "X:/partboot/generated");
    }
}
