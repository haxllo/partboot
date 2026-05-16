use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// ISO metadata cache entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IsoMetadataCache {
    pub iso_name: String,
    pub iso_size: u64,
    pub iso_mtime: u64, // seconds since UNIX_EPOCH
    pub vmlinuz_path: Option<String>,
    pub initrd_path: Option<String>,
    pub rootfs_path: Option<String>,
    pub iso_family: String, // e.g., "Fedora", "Ubuntu", "Arch"
}

/// Get or create cache directory
pub fn cache_dir() -> Result<PathBuf, String> {
    let cache_path = if cfg!(windows) {
        // Windows: %LOCALAPPDATA%\partboot
        let app_data = std::env::var("LOCALAPPDATA")
            .map_err(|_| "LOCALAPPDATA environment variable not set".to_string())?;
        PathBuf::from(app_data).join("partboot")
    } else {
        // Unix: $XDG_CACHE_HOME/partboot or ~/.cache/partboot
        let cache_base = std::env::var("XDG_CACHE_HOME").ok().unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.cache", home)
        });
        PathBuf::from(cache_base).join("partboot")
    };

    fs::create_dir_all(&cache_path).map_err(|e| e.to_string())?;
    Ok(cache_path)
}

/// Generate cache key from ISO path: {iso_name}_{size}_{mtime}.json
fn cache_key_from_iso(iso_path: &Path) -> Result<String, String> {
    let name = iso_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid ISO path")?;

    let metadata = fs::metadata(iso_path).map_err(|e| e.to_string())?;
    let mtime = metadata
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    Ok(format!(
        "{}_{}_{}",
        name.replace(".iso", "").replace(".ISO", ""),
        metadata.len(),
        mtime
    ))
}

/// Load ISO metadata from cache
pub fn load_from_cache(iso_path: &Path) -> Result<Option<IsoMetadataCache>, String> {
    let cache_dir = cache_dir()?;
    let key = cache_key_from_iso(iso_path)?;
    let cache_file = cache_dir.join(format!("{}.json", key));

    if !cache_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&cache_file).map_err(|e| e.to_string())?;
    let cached: IsoMetadataCache =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse cache: {}", e))?;

    Ok(Some(cached))
}

/// Save ISO metadata to cache
pub fn save_to_cache(
    iso_path: &Path,
    vmlinuz: Option<String>,
    initrd: Option<String>,
    rootfs: Option<String>,
    iso_family: String,
) -> Result<(), String> {
    let cache_dir = cache_dir()?;
    let key = cache_key_from_iso(iso_path)?;
    let cache_file = cache_dir.join(format!("{}.json", key));

    let metadata = fs::metadata(iso_path).map_err(|e| e.to_string())?;
    let mtime = metadata
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let iso_name = iso_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let entry = IsoMetadataCache {
        iso_name,
        iso_size: metadata.len(),
        iso_mtime: mtime,
        vmlinuz_path: vmlinuz,
        initrd_path: initrd,
        rootfs_path: rootfs,
        iso_family,
    };

    let json = serde_json::to_string_pretty(&entry)
        .map_err(|e| format!("Failed to serialize cache: {}", e))?;

    fs::write(&cache_file, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear cache for a specific ISO
#[allow(dead_code)]
pub fn clear_cache_for_iso(iso_path: &Path) -> Result<(), String> {
    let cache_dir = cache_dir()?;
    let key = cache_key_from_iso(iso_path)?;
    let cache_file = cache_dir.join(format!("{}.json", key));

    if cache_file.exists() {
        fs::remove_file(&cache_file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// List all cached ISOs
#[allow(dead_code)]
pub fn list_cached_isos() -> Result<Vec<IsoMetadataCache>, String> {
    let cache_dir = cache_dir()?;
    let mut cached = Vec::new();

    if !cache_dir.exists() {
        return Ok(cached);
    }

    for entry in fs::read_dir(&cache_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(metadata) = serde_json::from_str::<IsoMetadataCache>(&content) {
                    cached.push(metadata);
                }
            }
        }
    }

    Ok(cached)
}
