mod extract;
mod grub;
mod iso;
mod layout;
mod profile;

use crate::extract::{extract_casper, mark_extracted_images};
use crate::grub::generate_grub_cfg;
use crate::iso::{scan_iso_dir, support_label};
use crate::layout::PartBootLayout;
use crate::profile::{
    count_profile_files, ensure_profile_for_iso_name, ensure_profiles_for_images,
    load_profiles_for_images,
};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Init {
        root: PathBuf,
    },
    Scan {
        root: PathBuf,
        json: bool,
    },
    GenerateMenu {
        root: PathBuf,
        partition_uuid: String,
        partition_label: Option<String>,
        include_diagnostics: bool,
        json: bool,
        output: Option<PathBuf>,
    },
    Extract {
        root: PathBuf,
        iso: String,
    },
    StageEfi {
        root: PathBuf,
        grub_x64: PathBuf,
        boot_x64: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    InstallEsp {
        root: PathBuf,
        esp: PathBuf,
        dry_run: bool,
        force: bool,
    },
    InstallFallback {
        root: PathBuf,
        esp: PathBuf,
        dry_run: bool,
        force: bool,
    },
    BootInstructions {
        esp: PathBuf,
    },
    Doctor {
        root: PathBuf,
        esp: Option<PathBuf>,
        json: bool,
    },
    GuidedTestFlow {
        root: PathBuf,
        esp: PathBuf,
        partition_uuid: String,
        partition_label: Option<String>,
        iso: Option<String>,
        include_diagnostics: bool,
        json: bool,
        dry_run_install: bool,
    },
    GuidedTestFlowInteractive {
        include_diagnostics: bool,
        dry_run_install: bool,
    },
    VolumeId {
        drive: String,
    },
    RecommendTestPartitions,
    Help,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_command(&args).and_then(run) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!();
            print_help();
            process::exit(2);
        }
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Init { root } => {
            let layout = PartBootLayout::new(root);
            layout.ensure().map_err(|error| error.to_string())?;
            println!("initialized {}", layout.root.display());
            println!("copy ISO files into {}", layout.isos.display());
        }
        Command::Scan { root, json } => {
            let layout = PartBootLayout::new(root);
            let images = scan_iso_dir(&layout.isos).map_err(|error| error.to_string())?;
            let created = ensure_profiles_for_images(&layout, &images)?;
            if json {
                let image_items: Vec<String> = images
                    .iter()
                    .map(|image| {
                        format!(
                            "{{\"name\":\"{}\",\"family\":\"{}\",\"support\":\"{}\"}}",
                            json_escape(&image.name),
                            json_escape(&format!("{:?}", image.family)),
                            json_escape(support_label(&image.support))
                        )
                    })
                    .collect();
                let created_items: Vec<String> = created
                    .iter()
                    .map(|path| format!("\"{}\"", json_escape(&path.to_string_lossy())))
                    .collect();
                println!(
                    "{{\"root\":\"{}\",\"images\":[{}],\"created_profiles\":[{}]}}",
                    json_escape(&layout.root.to_string_lossy()),
                    image_items.join(","),
                    created_items.join(",")
                );
            } else if images.is_empty() {
                println!("[ok] no ISO images found in {}", layout.isos.display());
            } else {
                println!("[ok] scanned {}", layout.isos.display());
                for image in images {
                    println!(
                        "- {} | {:?} | {}",
                        image.name,
                        image.family,
                        support_label(&image.support)
                    );
                }
                for profile in created {
                    println!("[ok] created profile {}", profile.display());
                }
            }
        }
        Command::GenerateMenu {
            root,
            partition_uuid,
            partition_label,
            include_diagnostics,
            json,
            output,
        } => {
            let layout = PartBootLayout::new(root);
            validate_partition_uuid_for_root(&layout.root, &partition_uuid)?;
            let mut images = scan_iso_dir(&layout.isos).map_err(|error| error.to_string())?;
            ensure_profiles_for_images(&layout, &images)?;
            mark_extracted_images(&layout, &mut images);
            let profiles = load_profiles_for_images(&layout, &images)?;
            let cfg = generate_grub_cfg(
                &images,
                &partition_uuid,
                partition_label.as_deref(),
                include_diagnostics,
                &profiles,
            );
            let output = output.unwrap_or_else(|| layout.grub_cfg_path());
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::write(&output, cfg).map_err(|error| error.to_string())?;
            if json {
                println!(
                    "{{\"root\":\"{}\",\"output\":\"{}\",\"partition_uuid\":\"{}\",\"partition_label\":\"{}\",\"include_diagnostics\":{},\"image_count\":{}}}",
                    json_escape(&layout.root.to_string_lossy()),
                    json_escape(&output.to_string_lossy()),
                    json_escape(&partition_uuid),
                    json_escape(partition_label.as_deref().unwrap_or("")),
                    if include_diagnostics { "true" } else { "false" },
                    images.len()
                );
            } else {
                println!("[ok] wrote {}", output.display());
            }
        }
        Command::Extract { root, iso } => {
            let layout = PartBootLayout::new(root);
            let extracted_id = extract_casper(&layout, &iso)?;
            if let Some(path) = ensure_profile_for_iso_name(&layout, &iso_name_from_arg(&iso))? {
                println!("created profile {}", path.display());
            }
            println!(
                "extracted Ubuntu Casper files to {}",
                layout.extracted.join(extracted_id).display()
            );
        }
        Command::StageEfi {
            root,
            grub_x64,
            boot_x64,
            output,
        } => {
            let layout = PartBootLayout::new(root);
            let staged = stage_efi(&layout, &grub_x64, boot_x64.as_ref(), output)?;
            println!("staged EFI files in {}", staged.display());
            println!("next: inspect the staged files before copying anything to a real ESP");
        }
        Command::InstallEsp {
            root,
            esp,
            dry_run,
            force,
        } => {
            install_esp(&PartBootLayout::new(root), &esp, dry_run, force)?;
        }
        Command::InstallFallback {
            root,
            esp,
            dry_run,
            force,
        } => {
            install_fallback(&PartBootLayout::new(root), &esp, dry_run, force)?;
        }
        Command::BootInstructions { esp } => {
            print_boot_instructions(&esp)?;
        }
        Command::Doctor { root, esp, json } => {
            let layout = PartBootLayout::new(root);
            let isos_status = status(&layout.isos);
            let cache_status = status(&layout.cache);
            let generated_status = status(&layout.generated);
            let ntfs_uuid_status = doctor_ntfs_uuid_status(&layout);
            let extracted_status = doctor_extracted_status(&layout)?;
            let profiles_status = doctor_profiles_status(&layout)?;
            let esp_status = doctor_esp_status(esp.as_ref());
            let fallback_status = doctor_fallback_status(esp.as_ref());
            if json {
                println!(
                    "{{\"root\":\"{}\",\"isos\":\"{}\",\"cache\":\"{}\",\"generated\":\"{}\",\"full_ntfs_uuid_present\":\"{}\",\"extracted_files_complete\":\"{}\",\"profiles_present\":\"{}\",\"esp_files_installed\":\"{}\",\"fallback_installed\":\"{}\"}}",
                    json_escape(&layout.root.to_string_lossy()),
                    isos_status,
                    cache_status,
                    generated_status,
                    json_escape(&ntfs_uuid_status),
                    json_escape(&extracted_status),
                    json_escape(&profiles_status),
                    json_escape(&esp_status),
                    json_escape(&fallback_status)
                );
            } else {
                println!("root: {}", layout.root.display());
                println!("isos: {}", isos_status);
                println!("cache: {}", cache_status);
                println!("generated: {}", generated_status);
                println!("full NTFS UUID present: {}", ntfs_uuid_status);
                println!("extracted files complete: {}", extracted_status);
                println!("profiles present: {}", profiles_status);
                println!("ESP files installed: {}", esp_status);
                println!("fallback installed: {}", fallback_status);
                println!("note: this MVP never edits partitions or firmware boot entries");
            }
        }
        Command::GuidedTestFlow {
            root,
            esp,
            partition_uuid,
            partition_label,
            iso,
            include_diagnostics,
            json,
            dry_run_install,
        } => {
            run_guided_test_flow(
                root,
                esp,
                partition_uuid,
                partition_label,
                iso,
                include_diagnostics,
                json,
                dry_run_install,
            )?;
        }
        Command::GuidedTestFlowInteractive {
            include_diagnostics,
            dry_run_install,
        } => {
            run_guided_test_flow_interactive(include_diagnostics, dry_run_install)?;
        }
        Command::VolumeId { drive } => {
            print_volume_id(&drive)?;
        }
        Command::RecommendTestPartitions => {
            print_partition_recommendation();
        }
        Command::Help => print_help(),
    }
    Ok(())
}

fn parse_command(args: &[String]) -> Result<Command, String> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        return Ok(Command::Help);
    }

    match args[0].as_str() {
        "init" => Ok(Command::Init {
            root: required_path(args, "--root")?,
        }),
        "scan" => Ok(Command::Scan {
            root: required_path(args, "--root")?,
            json: has_flag(args, "--json"),
        }),
        "generate-menu" => Ok(Command::GenerateMenu {
            root: required_path(args, "--root")?,
            partition_uuid: required_value(args, "--partition-uuid")?,
            partition_label: optional_value(args, "--partition-label"),
            include_diagnostics: has_flag(args, "--include-diagnostics"),
            json: has_flag(args, "--json"),
            output: optional_path(args, "--output"),
        }),
        "extract" => Ok(Command::Extract {
            root: required_path(args, "--root")?,
            iso: required_value(args, "--iso")?,
        }),
        "stage-efi" => Ok(Command::StageEfi {
            root: required_path(args, "--root")?,
            grub_x64: required_path(args, "--grub-x64")?,
            boot_x64: optional_path(args, "--boot-x64"),
            output: optional_path(args, "--output"),
        }),
        "install-esp" => Ok(Command::InstallEsp {
            root: required_path(args, "--root")?,
            esp: required_path(args, "--esp")?,
            dry_run: has_flag(args, "--dry-run"),
            force: has_flag(args, "--force"),
        }),
        "install-fallback" => Ok(Command::InstallFallback {
            root: required_path(args, "--root")?,
            esp: required_path(args, "--esp")?,
            dry_run: has_flag(args, "--dry-run"),
            force: has_flag(args, "--force"),
        }),
        "boot-instructions" => Ok(Command::BootInstructions {
            esp: required_path(args, "--esp")?,
        }),
        "doctor" => Ok(Command::Doctor {
            root: required_path(args, "--root")?,
            esp: optional_path(args, "--esp"),
            json: has_flag(args, "--json"),
        }),
        "guided-test-flow" => Ok(Command::GuidedTestFlow {
            root: required_path(args, "--root")?,
            esp: required_path(args, "--esp")?,
            partition_uuid: required_value(args, "--partition-uuid")?,
            partition_label: optional_value(args, "--partition-label"),
            iso: optional_value(args, "--iso"),
            include_diagnostics: has_flag(args, "--include-diagnostics"),
            json: has_flag(args, "--json"),
            dry_run_install: has_flag(args, "--dry-run-install"),
        }),
        "guided-test-flow-interactive" | "start" => Ok(Command::GuidedTestFlowInteractive {
            include_diagnostics: has_flag(args, "--include-diagnostics"),
            dry_run_install: has_flag(args, "--dry-run-install"),
        }),
        "volume-id" => Ok(Command::VolumeId {
            drive: required_value(args, "--drive")?,
        }),
        "recommend-test-partitions" => Ok(Command::RecommendTestPartitions),
        command => Err(format!("unknown command '{command}'")),
    }
}

fn required_path(args: &[String], flag: &str) -> Result<PathBuf, String> {
    required_value(args, flag).map(PathBuf::from)
}

fn optional_path(args: &[String], flag: &str) -> Option<PathBuf> {
    optional_value(args, flag).map(PathBuf::from)
}

fn required_value(args: &[String], flag: &str) -> Result<String, String> {
    optional_value(args, flag).ok_or_else(|| format!("missing required flag {flag}"))
}

fn optional_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn status(path: &PathBuf) -> &'static str {
    if path.exists() {
        "present"
    } else {
        "missing"
    }
}

fn iso_name_from_arg(value: &str) -> String {
    let path = PathBuf::from(value);
    path.file_name()
        .and_then(|entry| entry.to_str())
        .map(|entry| entry.to_string())
        .unwrap_or_else(|| value.to_string())
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn run_guided_test_flow(
    root: PathBuf,
    esp: PathBuf,
    partition_uuid: String,
    partition_label: Option<String>,
    iso: Option<String>,
    include_diagnostics: bool,
    json: bool,
    dry_run_install: bool,
) -> Result<(), String> {
    let layout = PartBootLayout::new(root);
    layout.ensure().map_err(|error| error.to_string())?;
    validate_partition_uuid_for_root(&layout.root, &partition_uuid)?;

    if !json {
        println!("[step] init");
        println!("[ok] initialized {}", layout.root.display());
    }

    let mut images = scan_iso_dir(&layout.isos).map_err(|error| error.to_string())?;
    let imported_drive_isos = if images.is_empty() {
        import_drive_root_isos(&layout)?
    } else {
        Vec::new()
    };
    if !imported_drive_isos.is_empty() {
        images = scan_iso_dir(&layout.isos).map_err(|error| error.to_string())?;
        if !json {
            println!("[step] import");
            println!(
                "[ok] imported {} ISO file(s) from drive root into {}",
                imported_drive_isos.len(),
                layout.isos.display()
            );
        }
    }
    if images.is_empty() {
        return Err(format!(
            "no ISO images found in {}; copy ISO files and rerun",
            layout.isos.display()
        ));
    }
    let created_profiles = ensure_profiles_for_images(&layout, &images)?;
    if !json {
        println!("[step] scan");
        println!("[ok] found {} ISO image(s)", images.len());
    }

    let selected_iso = if let Some(iso_name) = iso {
        Some(iso_name)
    } else {
        images
            .iter()
            .find(|image| matches!(image.family, crate::iso::IsoFamily::UbuntuCasper))
            .map(|image| image.name.clone())
    };

    let extracted_target = if let Some(iso_name) = selected_iso {
        let extract_label = format!("extract {}", iso_name);
        run_with_spinner(!json, &extract_label, || {
            extract_casper(&layout, &iso_name)?;
            ensure_profile_for_iso_name(&layout, &iso_name)?;
            Ok(())
        })?;
        if !json {
            println!("[ok] extracted {}", iso_name);
        }
        Some(iso_name)
    } else {
        if !json {
            println!("[warn] no Ubuntu ISO found; skipping extract step");
        }
        None
    };

    images = scan_iso_dir(&layout.isos).map_err(|error| error.to_string())?;
    mark_extracted_images(&layout, &mut images);
    let profiles = load_profiles_for_images(&layout, &images)?;
    let generated_cfg = layout.grub_cfg_path();
    run_with_spinner(!json, "generate-menu", || {
        let cfg = generate_grub_cfg(
            &images,
            &partition_uuid,
            partition_label.as_deref(),
            include_diagnostics,
            &profiles,
        );
        if let Some(parent) = generated_cfg.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&generated_cfg, cfg).map_err(|error| error.to_string())?;
        Ok(())
    })?;
    if !json {
        println!("[ok] wrote {}", generated_cfg.display());
    }

    let efi_binaries = resolve_efi_binaries_for_stage(&layout)?;
    if !json && efi_binaries.copied_from_bundle {
        println!("[step] cache");
        println!(
            "[ok] populated cache binaries from {}",
            efi_binaries.source
        );
    }
    let staged = stage_efi(
        &layout,
        &efi_binaries.grub_x64,
        Some(&efi_binaries.boot_x64),
        None,
    )?;
    if !json {
        println!("[step] stage-efi");
        println!("[ok] staged {}", staged.display());
    }

    install_esp(&layout, &esp, dry_run_install, !dry_run_install)?;
    install_fallback(&layout, &esp, dry_run_install, !dry_run_install)?;
    if !json {
        println!("[step] install");
        if dry_run_install {
            println!("[ok] install steps executed in dry-run mode");
        } else {
            println!("[ok] installed to {}", esp.display());
        }
    }

    let ntfs_uuid_status = doctor_ntfs_uuid_status(&layout);
    let extracted_status = doctor_extracted_status(&layout)?;
    let profiles_status = doctor_profiles_status(&layout)?;
    let esp_status = doctor_esp_status(Some(&esp));
    let fallback_status = doctor_fallback_status(Some(&esp));

    if json {
        let created_items: Vec<String> = created_profiles
            .iter()
            .map(|path| format!("\"{}\"", json_escape(&path.to_string_lossy())))
            .collect();
        println!(
            "{{\"root\":\"{}\",\"esp\":\"{}\",\"partition_uuid\":\"{}\",\"partition_label\":\"{}\",\"image_count\":{},\"imported_drive_root_isos\":{},\"created_profiles\":[{}],\"extracted_iso\":\"{}\",\"generated_cfg\":\"{}\",\"staged_dir\":\"{}\",\"efi_binary_source\":\"{}\",\"dry_run_install\":{},\"doctor\":{{\"full_ntfs_uuid_present\":\"{}\",\"extracted_files_complete\":\"{}\",\"profiles_present\":\"{}\",\"esp_files_installed\":\"{}\",\"fallback_installed\":\"{}\"}}}}",
            json_escape(&layout.root.to_string_lossy()),
            json_escape(&esp.to_string_lossy()),
            json_escape(&partition_uuid),
            json_escape(partition_label.as_deref().unwrap_or("")),
            images.len(),
            imported_drive_isos.len(),
            created_items.join(","),
            json_escape(extracted_target.as_deref().unwrap_or("")),
            json_escape(&generated_cfg.to_string_lossy()),
            json_escape(&staged.to_string_lossy()),
            json_escape(&efi_binaries.source),
            if dry_run_install { "true" } else { "false" },
            json_escape(&ntfs_uuid_status),
            json_escape(&extracted_status),
            json_escape(&profiles_status),
            json_escape(&esp_status),
            json_escape(&fallback_status)
        );
    } else {
        println!("[step] doctor");
        println!("[ok] full NTFS UUID present: {}", ntfs_uuid_status);
        println!("[ok] extracted files complete: {}", extracted_status);
        println!("[ok] profiles present: {}", profiles_status);
        println!("[ok] ESP files installed: {}", esp_status);
        println!("[ok] fallback installed: {}", fallback_status);
    }

    Ok(())
}

fn import_drive_root_isos(layout: &PartBootLayout) -> Result<Vec<PathBuf>, String> {
    let drive = drive_from_path(&layout.root)?;
    let drive_root = PathBuf::from(format!("{drive}\\"));
    if !drive_root.exists() {
        return Ok(Vec::new());
    }
    fs::create_dir_all(&layout.isos).map_err(|error| error.to_string())?;

    let mut imported = Vec::new();
    let entries = fs::read_dir(&drive_root).map_err(|error| error.to_string())?;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if !path.is_file() {
            continue;
        }
        let is_iso = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("iso"))
            .unwrap_or(false);
        if !is_iso {
            continue;
        }
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let destination = layout.isos.join(file_name);
        if destination.exists() {
            continue;
        }
        import_iso_from_drive_root(&path, &destination)?;
        imported.push(destination);
    }
    Ok(imported)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportMode {
    Moved,
    Copied,
}

fn import_iso_from_drive_root(source: &Path, destination: &Path) -> Result<ImportMode, String> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(ImportMode::Moved),
        Err(rename_error) => fs::copy(source, destination)
            .map(|_| ImportMode::Copied)
            .map_err(|copy_error| {
                format!(
                    "failed importing {} to {}: rename failed ({}) and copy failed ({})",
                    source.display(),
                    destination.display(),
                    rename_error,
                    copy_error
                )
            }),
    }
}

fn run_with_spinner<T, F>(enabled: bool, label: &str, operation: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    if !enabled {
        return operation();
    }

    let label_owned = label.to_string();
    let done = Arc::new(AtomicBool::new(false));
    let done_thread = Arc::clone(&done);
    let spinner_label = label_owned.clone();
    let spinner = thread::spawn(move || {
        let frames = ['|', '/', '-', '\\'];
        let mut index = 0usize;
        while !done_thread.load(Ordering::Relaxed) {
            let _ = write!(
                io::stdout(),
                "\r[work] {} {}",
                spinner_label,
                frames[index % frames.len()]
            );
            let _ = io::stdout().flush();
            index = index.wrapping_add(1);
            thread::sleep(Duration::from_millis(120));
        }
    });

    let result = operation();
    done.store(true, Ordering::Relaxed);
    let _ = spinner.join();

    let clear_width = label_owned.len() + 16;
    let _ = write!(io::stdout(), "\r{}\r", " ".repeat(clear_width));
    let _ = io::stdout().flush();

    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EfiBinaryPaths {
    grub_x64: PathBuf,
    boot_x64: PathBuf,
    source: String,
    copied_from_bundle: bool,
}

fn resolve_efi_binaries_for_stage(layout: &PartBootLayout) -> Result<EfiBinaryPaths, String> {
    let cache_grub = layout.cache.join("grubx64.efi");
    let cache_boot = layout.cache.join("bootx64.efi");
    if cache_grub.exists() && cache_boot.exists() {
        return Ok(EfiBinaryPaths {
            grub_x64: cache_grub,
            boot_x64: cache_boot,
            source: layout.cache.display().to_string(),
            copied_from_bundle: false,
        });
    }

    if let Some(bundled_dir) = locate_bundled_efi_dir() {
        return populate_cache_from_verified_assets(layout, &bundled_dir, &cache_grub, &cache_boot);
    }

    if let Some(downloaded) = download_release_efi_assets(layout)? {
        return populate_cache_from_verified_assets(
            layout,
            &downloaded.dir,
            &cache_grub,
            &cache_boot,
        )
        .map(|paths| EfiBinaryPaths {
            source: downloaded.source,
            copied_from_bundle: paths.copied_from_bundle,
            grub_x64: paths.grub_x64,
            boot_x64: paths.boot_x64,
        });
    }

    Err(format!(
        "missing staged binaries in {} and no bundled EFI assets found. Expected grubx64.efi and bootx64.efi in cache, packaged assets under assets\\efi, or GitHub release fallback assets",
        layout.cache.display()
    ))
}

fn locate_bundled_efi_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(configured) = env::var("PARTBOOT_EFI_ASSETS") {
        let configured = configured.trim();
        if !configured.is_empty() {
            let configured_path = PathBuf::from(configured);
            candidates.push(configured_path.clone());
            candidates.push(configured_path.join("efi"));
        }
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(exe_dir.join("assets").join("efi"));
            if let Some(parent) = exe_dir.parent() {
                candidates.push(parent.join("assets").join("efi"));
            }
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        candidates.push(current_dir.join("assets").join("efi"));
    }

    candidates
        .into_iter()
        .find(|candidate| has_required_efi_files(candidate))
}

fn has_required_efi_files(dir: &Path) -> bool {
    dir.join("grubx64.efi").exists() && dir.join("bootx64.efi").exists()
}

fn populate_cache_from_verified_assets(
    layout: &PartBootLayout,
    source_dir: &Path,
    cache_grub: &PathBuf,
    cache_boot: &PathBuf,
) -> Result<EfiBinaryPaths, String> {
    verify_bundled_efi_checksums(source_dir)?;
    fs::create_dir_all(&layout.cache).map_err(|error| error.to_string())?;
    fs::copy(source_dir.join("grubx64.efi"), cache_grub).map_err(|error| {
        format!(
            "failed to copy grubx64.efi into {}: {}",
            cache_grub.display(),
            error
        )
    })?;
    fs::copy(source_dir.join("bootx64.efi"), cache_boot).map_err(|error| {
        format!(
            "failed to copy bootx64.efi into {}: {}",
            cache_boot.display(),
            error
        )
    })?;

    Ok(EfiBinaryPaths {
        grub_x64: cache_grub.clone(),
        boot_x64: cache_boot.clone(),
        source: source_dir.display().to_string(),
        copied_from_bundle: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DownloadedEfiAssets {
    dir: PathBuf,
    source: String,
}

fn download_release_efi_assets(layout: &PartBootLayout) -> Result<Option<DownloadedEfiAssets>, String> {
    #[cfg(not(windows))]
    {
        let _ = layout;
        Ok(None)
    }

    #[cfg(windows)]
    {
        let base = env::var("PARTBOOT_EFI_RELEASE_BASE")
            .unwrap_or_else(|_| "https://github.com/haxllo/partboot/releases/download".to_string());
        let base = base.trim().trim_end_matches('/').to_string();
        if base.is_empty() {
            return Err("PARTBOOT_EFI_RELEASE_BASE is set but empty".to_string());
        }
        let tag = env::var("PARTBOOT_EFI_RELEASE_TAG")
            .unwrap_or_else(|_| format!("v{}", env!("CARGO_PKG_VERSION")));
        let tag = tag.trim().to_string();
        if tag.is_empty() {
            return Err("PARTBOOT_EFI_RELEASE_TAG is set but empty".to_string());
        }
        let source = format!("{base}/{tag}");
        let download_dir = layout.cache.join("_release_fallback");
        fs::create_dir_all(&download_dir).map_err(|error| {
            format!(
                "failed to create release fallback directory {}: {}",
                download_dir.display(),
                error
            )
        })?;

        for file in ["grubx64.efi", "bootx64.efi", "checksums.txt"] {
            let url = format!("{source}/{file}");
            let destination = download_dir.join(file);
            download_file_powershell(&url, &destination).map_err(|error| {
                format!(
                    "{}; set PARTBOOT_EFI_RELEASE_BASE and PARTBOOT_EFI_RELEASE_TAG to override source",
                    error
                )
            })?;
        }

        Ok(Some(DownloadedEfiAssets {
            dir: download_dir,
            source,
        }))
    }
}

#[cfg(windows)]
fn download_file_powershell(url: &str, destination: &Path) -> Result<(), String> {
    let escaped_url = url.replace('\'', "''");
    let escaped_path = destination.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; Invoke-WebRequest -UseBasicParsing -Uri '{}' -OutFile '{}'",
        escaped_url, escaped_path
    );
    let output = process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|error| format!("failed to invoke PowerShell for {}: {}", url, error))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "unknown download failure".to_string()
        };
        Err(format!("failed downloading {}: {}", url, details))
    }
}

fn verify_bundled_efi_checksums(dir: &Path) -> Result<(), String> {
    let manifest_path = dir.join("checksums.txt");
    let manifest = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "missing or unreadable checksum manifest {}: {}",
            manifest_path.display(),
            error
        )
    })?;
    let checksums = parse_checksum_manifest(&manifest)?;
    verify_checksum_entry(dir, &checksums, "grubx64.efi")?;
    verify_checksum_entry(dir, &checksums, "bootx64.efi")?;
    Ok(())
}

fn verify_checksum_entry(
    dir: &Path,
    checksums: &[(String, String)],
    file_name: &str,
) -> Result<(), String> {
    let expected = checksums
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(file_name))
        .map(|(_, checksum)| checksum.clone())
        .ok_or_else(|| format!("checksum entry missing for {file_name}"))?;
    let file_path = dir.join(file_name);
    let actual = file_crc32_hex(&file_path)?;
    if actual.eq_ignore_ascii_case(&expected) {
        Ok(())
    } else {
        Err(format!(
            "checksum mismatch for {}: expected {}, got {}",
            file_path.display(),
            expected,
            actual
        ))
    }
}

fn parse_checksum_manifest(content: &str) -> Result<Vec<(String, String)>, String> {
    let mut entries = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (name, checksum) = trimmed
            .split_once('=')
            .ok_or_else(|| format!("invalid checksum line: {trimmed}"))?;
        let name = name.trim().to_string();
        let checksum = checksum.trim().to_ascii_uppercase();
        if name.is_empty() || checksum.is_empty() {
            return Err(format!("invalid checksum line: {trimmed}"));
        }
        if !checksum.chars().all(|ch| ch.is_ascii_hexdigit()) || checksum.len() != 8 {
            return Err(format!("invalid checksum for {name}: {checksum}"));
        }
        entries.push((name, checksum));
    }
    Ok(entries)
}

fn file_crc32_hex(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read {}: {}", path.display(), error))?;
    Ok(format!("{:08X}", crc32(&bytes)))
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = if crc & 1 == 1 { 0xEDB8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsVolume {
    drive: String,
    filesystem: String,
    label: Option<String>,
}

fn run_guided_test_flow_interactive(
    include_diagnostics: bool,
    dry_run_install: bool,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = include_diagnostics;
        let _ = dry_run_install;
        return Err("start/guided-test-flow-interactive is currently Windows only".to_string());
    }

    #[cfg(windows)]
    {
        let volumes = list_windows_volumes()?;
        let root_candidates: Vec<WindowsVolume> = volumes
            .iter()
            .filter(|volume| volume.filesystem.eq_ignore_ascii_case("NTFS"))
            .cloned()
            .collect();
        let esp_candidates: Vec<WindowsVolume> = volumes
            .iter()
            .filter(|volume| volume.filesystem.eq_ignore_ascii_case("FAT32"))
            .cloned()
            .collect();

        if root_candidates.is_empty() {
            return Err("no NTFS partitions detected for root selection".to_string());
        }
        if esp_candidates.is_empty() {
            return Err("no FAT32 partitions detected for ESP selection".to_string());
        }

        println!("[interactive] Select NTFS partition for PartBoot root:");
        let root_choice = choose_volume(&root_candidates)?;
        println!("[interactive] Select FAT32 partition for ESP:");
        let esp_choice = choose_volume(&esp_candidates)?;

        let root = PathBuf::from(format!("{}\\partboot", root_choice.drive));
        let esp = PathBuf::from(format!("{}\\", esp_choice.drive));

        let partition_uuid = detect_partition_uuid(&root_choice.drive)?;
        let auto_label = root_choice.label.unwrap_or_default();
        let partition_label = prompt_partition_label(&auto_label)?;

        println!("[interactive] detected root: {}", root.display());
        println!("[interactive] detected esp: {}", esp.display());
        println!("[interactive] detected partition uuid: {}", partition_uuid);
        println!(
            "[interactive] using partition label: {}",
            partition_label.as_deref().unwrap_or("(none)")
        );

        run_guided_test_flow(
            root,
            esp,
            partition_uuid,
            partition_label,
            None,
            include_diagnostics,
            false,
            dry_run_install,
        )
    }
}

fn choose_volume(volumes: &[WindowsVolume]) -> Result<WindowsVolume, String> {
    for (index, volume) in volumes.iter().enumerate() {
        let label = volume.label.as_deref().unwrap_or("(no-label)");
        println!(
            "  {}. {} | {} | {}",
            index + 1,
            volume.drive,
            volume.filesystem,
            label
        );
    }

    print!("Select number: ");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| error.to_string())?;
    let choice = input
        .trim()
        .parse::<usize>()
        .map_err(|_| "invalid selection; expected a number".to_string())?;
    if choice == 0 || choice > volumes.len() {
        return Err("selection out of range".to_string());
    }
    Ok(volumes[choice - 1].clone())
}

fn prompt_partition_label(detected: &str) -> Result<Option<String>, String> {
    if detected.is_empty() {
        print!("Partition label not detected. Enter label (optional): ");
    } else {
        print!(
            "Detected partition label '{}' (press Enter to accept, or type a new label): ",
            detected
        );
    }
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| error.to_string())?;
    let entered = input.trim();
    if entered.is_empty() {
        if detected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(detected.to_string()))
        }
    } else {
        Ok(Some(entered.to_string()))
    }
}

fn detect_partition_uuid(drive: &str) -> Result<String, String> {
    #[cfg(not(windows))]
    {
        let _ = drive;
        return Err("partition UUID detection is currently Windows only".to_string());
    }

    #[cfg(windows)]
    {
        let filesystem = query_cim_volume(drive)?
            .map(|info| info.filesystem)
            .unwrap_or_default();

        if let Ok(output) = run_fsutil(&["fsinfo", "ntfsinfo", drive]) {
            if let Some(uuid) = parse_ntfs_serial(&output) {
                return Ok(uuid);
            }
        }

        if filesystem.eq_ignore_ascii_case("NTFS") {
            return Err(format!(
                "full NTFS UUID is required for {drive}. Run elevated: fsutil fsinfo ntfsinfo {drive} and use the NTFS Volume Serial Number without 0x"
            ));
        }

        if let Ok(output) = run_fsutil(&["fsinfo", "volumeinfo", drive]) {
            if let Some(serial) = parse_volume_serial(&output) {
                return Ok(serial.replace('-', ""));
            }
        }
        if let Some(info) = query_cim_volume(drive)? {
            return Ok(info.serial.replace('-', ""));
        }
        Err(format!("could not detect partition UUID for {drive}"))
    }
}

fn validate_partition_uuid_for_root(root: &PathBuf, partition_uuid: &str) -> Result<(), String> {
    if !is_full_hex_uuid(partition_uuid) {
        #[cfg(windows)]
        {
            if let Ok(drive) = drive_from_path(root) {
                if let Some(info) = query_cim_volume(&drive)? {
                    if info.filesystem.eq_ignore_ascii_case("NTFS") {
                        return Err(format!(
                            "partition UUID '{}' looks short for NTFS on {}. Full NTFS UUID is required; run elevated: fsutil fsinfo ntfsinfo {}",
                            partition_uuid, drive, drive
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn is_full_hex_uuid(value: &str) -> bool {
    let compact: String = value.chars().filter(|ch| *ch != '-').collect();
    compact.len() >= 16 && compact.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn list_windows_volumes() -> Result<Vec<WindowsVolume>, String> {
    #[cfg(not(windows))]
    {
        return Err("volume detection is currently Windows only".to_string());
    }

    #[cfg(windows)]
    {
        let command = "$vols = Get-CimInstance Win32_LogicalDisk | Where-Object {$_.DriveType -eq 3}; foreach ($v in $vols) { \"$($v.DeviceID)|$($v.FileSystem)|$($v.VolumeName)\" }";
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", command])
            .output()
            .map_err(|error| format!("failed to detect Windows volumes: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                "failed to detect Windows volumes".to_string()
            } else {
                stderr
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut volumes = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut parts = trimmed.splitn(3, '|');
            let drive = parts.next().unwrap_or("").trim().to_string();
            let filesystem = parts.next().unwrap_or("").trim().to_string();
            let label = parts.next().map(|value| value.trim().to_string());
            if drive.is_empty() || filesystem.is_empty() {
                continue;
            }
            volumes.push(WindowsVolume {
                drive,
                filesystem,
                label: label.filter(|value| !value.is_empty()),
            });
        }
        Ok(volumes)
    }
}

fn doctor_ntfs_uuid_status(layout: &PartBootLayout) -> String {
    let cfg = match fs::read_to_string(layout.grub_cfg_path()) {
        Ok(content) => content,
        Err(_) => return "no (missing generated\\grub.cfg)".to_string(),
    };
    const PREFIX: &str = "search --no-floppy --fs-uuid --set=partboot_root ";
    for line in cfg.lines() {
        if let Some(value) = line.strip_prefix(PREFIX) {
            let compact: String = value
                .trim()
                .trim_matches('\'')
                .chars()
                .filter(|ch| *ch != '-')
                .collect();
            if compact.len() >= 16 && compact.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return "yes".to_string();
            }
            return "no".to_string();
        }
    }
    "no".to_string()
}

fn doctor_extracted_status(layout: &PartBootLayout) -> Result<String, String> {
    let images = scan_iso_dir(&layout.isos).map_err(|error| error.to_string())?;
    let ubuntu_images: Vec<_> = images
        .into_iter()
        .filter(|image| matches!(image.family, crate::iso::IsoFamily::UbuntuCasper))
        .collect();
    if ubuntu_images.is_empty() {
        return Ok("n/a (no Ubuntu ISO found)".to_string());
    }

    let complete = ubuntu_images
        .iter()
        .filter(|image| {
            crate::extract::is_complete_extracted_casper(
                layout,
                &crate::extract::extracted_id_from_iso_name(&image.name),
            )
        })
        .count();
    if complete == ubuntu_images.len() {
        Ok("yes".to_string())
    } else {
        Ok(format!("no ({complete}/{})", ubuntu_images.len()))
    }
}

fn doctor_profiles_status(layout: &PartBootLayout) -> Result<String, String> {
    let count = count_profile_files(layout)?;
    if count > 0 {
        Ok("yes".to_string())
    } else {
        Ok("no".to_string())
    }
}

fn doctor_esp_status(esp: Option<&PathBuf>) -> String {
    let Some(esp) = esp else {
        return "n/a (pass --esp <path>)".to_string();
    };
    let partboot = esp.join("EFI").join("PartBoot");
    if partboot.join("grubx64.efi").exists() && partboot.join("grub.cfg").exists() {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn doctor_fallback_status(esp: Option<&PathBuf>) -> String {
    let Some(esp) = esp else {
        return "n/a (pass --esp <path>)".to_string();
    };
    let fallback = esp.join("EFI").join("Boot");
    if fallback.join("bootx64.efi").exists() && fallback.join("grub.cfg").exists() {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn install_esp(
    layout: &PartBootLayout,
    esp: &PathBuf,
    dry_run: bool,
    force: bool,
) -> Result<(), String> {
    if !dry_run && !force {
        return Err("install-esp requires --dry-run or --force".to_string());
    }

    if !esp.exists() {
        return Err(format!("ESP path does not exist: {}", esp.display()));
    }

    let staged = layout.root.join("efi").join("EFI").join("PartBoot");
    let staged_grub = staged.join("grubx64.efi");
    let staged_boot = staged.join("bootx64.efi");
    let staged_cfg = staged.join("grub.cfg");
    if !staged_grub.exists() || !staged_cfg.exists() {
        return Err(format!(
            "staged EFI files not found in {}; run stage-efi first",
            staged.display()
        ));
    }

    validate_esp_filesystem(esp)?;

    let destination = esp.join("EFI").join("PartBoot");
    println!("source: {}", staged.display());
    println!("destination: {}", destination.display());

    if dry_run {
        println!("dry-run: would create {}", destination.display());
        println!("dry-run: would copy grubx64.efi");
        println!("dry-run: would copy grub.cfg");
        println!("dry-run: no files changed");
        return Ok(());
    }

    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    fs::copy(&staged_grub, destination.join("grubx64.efi")).map_err(|error| error.to_string())?;
    if staged_boot.exists() {
        fs::copy(&staged_boot, destination.join("bootx64.efi"))
            .map_err(|error| error.to_string())?;
    }
    fs::copy(&staged_cfg, destination.join("grub.cfg")).map_err(|error| error.to_string())?;
    fs::write(
        destination.join("README.txt"),
        "PartBoot EFI files.\r\nThis directory was created by partboot install-esp.\r\n",
    )
    .map_err(|error| error.to_string())?;
    println!("installed EFI files in {}", destination.display());
    println!("firmware boot entries were not modified");
    Ok(())
}

fn install_fallback(
    layout: &PartBootLayout,
    esp: &PathBuf,
    dry_run: bool,
    force: bool,
) -> Result<(), String> {
    if !dry_run && !force {
        return Err("install-fallback requires --dry-run or --force".to_string());
    }

    validate_esp_filesystem(esp)?;

    let staged = layout.root.join("efi").join("EFI").join("PartBoot");
    let staged_boot = staged.join("bootx64.efi");
    let staged_grub = staged.join("grubx64.efi");
    let staged_cfg = staged.join("grub.cfg");
    if !staged_boot.exists() {
        return Err(format!(
            "staged bootx64.efi not found in {}; rerun stage-efi with --boot-x64",
            staged.display()
        ));
    }
    if !staged_grub.exists() || !staged_cfg.exists() {
        return Err(format!(
            "staged GRUB files not found in {}; run stage-efi first",
            staged.display()
        ));
    }

    let destination = esp.join("EFI").join("Boot");
    let destination_boot = destination.join("bootx64.efi");
    println!("source: {}", staged.display());
    println!("fallback destination: {}", destination.display());

    if destination_boot.exists() && !dry_run && !force {
        return Err(format!(
            "{} already exists; rerun with --force only if this is a disposable ESP",
            destination_boot.display()
        ));
    }

    if dry_run {
        println!("dry-run: would create {}", destination.display());
        println!("dry-run: would copy bootx64.efi");
        println!("dry-run: would copy grubx64.efi");
        println!("dry-run: would copy grub.cfg");
        println!("dry-run: no files changed");
        return Ok(());
    }

    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    fs::copy(&staged_boot, destination_boot).map_err(|error| error.to_string())?;
    fs::copy(&staged_grub, destination.join("grubx64.efi")).map_err(|error| error.to_string())?;
    fs::copy(&staged_cfg, destination.join("grub.cfg")).map_err(|error| error.to_string())?;
    fs::write(
        destination.join("README.txt"),
        "PartBoot UEFI fallback files.\r\nCreated by partboot install-fallback.\r\n",
    )
    .map_err(|error| error.to_string())?;
    println!("installed fallback EFI files in {}", destination.display());
    println!("next: reboot and choose the UEFI entry for this disk/partition");
    Ok(())
}

fn validate_esp_filesystem(esp: &PathBuf) -> Result<(), String> {
    #[cfg(windows)]
    {
        let drive = drive_from_path(esp)?;
        if let Some(info) = query_cim_volume(&drive)? {
            if info.filesystem.eq_ignore_ascii_case("FAT32") {
                return Ok(());
            }
            return Err(format!(
                "{} is {}, expected FAT32 for UEFI firmware readability",
                drive, info.filesystem
            ));
        }
        Err(format!("could not determine filesystem for {drive}"))
    }

    #[cfg(not(windows))]
    {
        let _ = esp;
        Ok(())
    }
}

fn print_boot_instructions(esp: &PathBuf) -> Result<(), String> {
    validate_esp_filesystem(esp)?;
    let partboot_dir = esp.join("EFI").join("PartBoot");
    let loader = partboot_dir.join("grubx64.efi");
    let shim = partboot_dir.join("bootx64.efi");
    let cfg = partboot_dir.join("grub.cfg");

    if !loader.exists() {
        return Err(format!("missing {}", loader.display()));
    }
    if !cfg.exists() {
        return Err(format!("missing {}", cfg.display()));
    }

    println!("Manual UEFI boot test:");
    println!("1. Reboot and open your firmware boot menu.");
    println!("2. Choose the firmware option to boot from an EFI file if available.");
    if shim.exists() {
        println!("3. Select this file first, especially if Secure Boot is enabled:");
        println!("   {}", shim.display());
        println!("4. If that fails, disable Secure Boot temporarily and try:");
        println!("   {}", loader.display());
        println!("5. If your firmware shows paths relative to the ESP, choose:");
        println!("   \\EFI\\PartBoot\\bootx64.efi");
    } else {
        println!("3. Select this file:");
        println!("   {}", loader.display());
        println!("4. If your firmware shows paths relative to the ESP, choose:");
        println!("   \\EFI\\PartBoot\\grubx64.efi");
    }
    println!("6. No firmware boot entry has been created by PartBoot yet.");
    println!();
    println!("Expected result:");
    println!("   GRUB opens and shows your ISO entries.");
    println!();
    println!("If GRUB opens but cannot find the ISO partition:");
    println!("   Regenerate grub.cfg with the full NTFS serial from an elevated shell.");
    Ok(())
}

fn drive_from_path(path: &PathBuf) -> Result<String, String> {
    let value = path.to_string_lossy();
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        Ok(format!("{}:", (bytes[0] as char).to_ascii_uppercase()))
    } else {
        Err(format!(
            "ESP path must start with a drive letter, got {}",
            path.display()
        ))
    }
}

fn stage_efi(
    layout: &PartBootLayout,
    grub_x64: &PathBuf,
    boot_x64: Option<&PathBuf>,
    output: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if !grub_x64.exists() {
        return Err(format!("GRUB EFI binary not found: {}", grub_x64.display()));
    }

    let source_cfg = layout.grub_cfg_path();
    if !source_cfg.exists() {
        return Err(format!(
            "generated GRUB config not found: {}; run generate-menu first",
            source_cfg.display()
        ));
    }

    let stage_root = output.unwrap_or_else(|| layout.root.join("efi"));
    let partboot_efi = stage_root.join("EFI").join("PartBoot");
    fs::create_dir_all(&partboot_efi).map_err(|error| error.to_string())?;

    fs::copy(grub_x64, partboot_efi.join("grubx64.efi")).map_err(|error| error.to_string())?;
    if let Some(boot_x64) = boot_x64 {
        if !boot_x64.exists() {
            return Err(format!(
                "BOOTX64 EFI binary not found: {}",
                boot_x64.display()
            ));
        }
        fs::copy(boot_x64, partboot_efi.join("bootx64.efi")).map_err(|error| error.to_string())?;
    }
    fs::copy(&source_cfg, partboot_efi.join("grub.cfg")).map_err(|error| error.to_string())?;
    fs::write(
        partboot_efi.join("README.txt"),
        "PartBoot staged EFI files.\r\nThese files are safe staging output only.\r\nDo not copy to a real EFI System Partition until manual review.\r\n",
    )
    .map_err(|error| error.to_string())?;

    Ok(partboot_efi)
}

fn print_partition_recommendation() {
    println!("Recommended testing setup:");
    println!("1. Create one disposable 16-64 GB NTFS partition first.");
    println!("2. Put PartBoot at <drive>:\\partboot and test scan/menu generation there.");
    println!("3. Add FAT32 later only for EFI-file experiments; it cannot store ISOs over 4 GB.");
    println!("4. Add ext4 later for Linux-first testing; Windows will not manage it comfortably.");
    println!("5. Do not test on a partition containing personal data or an existing OS.");
}

fn print_volume_id(drive: &str) -> Result<(), String> {
    let drive = normalize_drive(drive)?;

    #[cfg(windows)]
    {
        let ntfsinfo = run_fsutil(&["fsinfo", "ntfsinfo", &drive]).unwrap_or_default();
        if let Some(uuid) = parse_ntfs_serial(&ntfsinfo) {
            println!("drive: {drive}");
            println!("filesystem: NTFS");
            println!("windows-serial: 0x{uuid}");
            println!("grub-fs-uuid-candidate: {uuid}");
            println!("use: --partition-uuid {uuid}");
            return Ok(());
        }

        if let Ok(volumeinfo) = run_fsutil(&["fsinfo", "volumeinfo", &drive]) {
            if let Some(serial) = parse_volume_serial(&volumeinfo) {
                println!("drive: {drive}");
                println!("windows-serial: {serial}");
                println!("grub-fs-uuid-candidate: {}", serial.replace('-', ""));
                println!("if FAT32, GRUB may prefer the dashed form: {serial}");
                return Ok(());
            }
        }

        if let Some(info) = query_cim_volume(&drive)? {
            println!("drive: {drive}");
            println!("filesystem: {}", info.filesystem);
            if info.filesystem.eq_ignore_ascii_case("NTFS") {
                println!("windows-short-serial: {}", info.serial);
                println!("full-ntfs-uuid-required: true");
                println!("run elevated: fsutil fsinfo ntfsinfo {drive}");
                println!("use the NTFS Volume Serial Number without the 0x prefix");
            } else {
                println!("windows-serial: {}", info.serial);
                println!("grub-fs-uuid-candidate: {}", info.serial.replace('-', ""));
            }
            return Ok(());
        }

        Err("could not find a volume serial number".to_string())
    }

    #[cfg(not(windows))]
    {
        let _ = drive;
        Err("volume-id currently supports Windows only".to_string())
    }
}

#[cfg(windows)]
fn run_fsutil(args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("fsutil")
        .args(args)
        .output()
        .map_err(|error| format!("failed to run fsutil: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stderr.is_empty() { stdout } else { stderr };
        if is_access_denied_message(&message) {
            return run_fsutil_elevated(args);
        }
        return Err(if message.is_empty() {
            format!("fsutil exited with status {}", output.status)
        } else {
            message
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(windows)]
fn run_fsutil_elevated(args: &[&str]) -> Result<String, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let out_path = std::env::temp_dir().join(format!("partboot-fsutil-out-{unique}.txt"));
    let err_path = std::env::temp_dir().join(format!("partboot-fsutil-err-{unique}.txt"));
    let out_text_path = out_path.to_string_lossy().to_string();
    let err_text_path = err_path.to_string_lossy().to_string();
    let arg_list = format!(
        "/c fsutil {} 1>\"{}\" 2>\"{}\"",
        args.join(" "),
        out_text_path,
        err_text_path
    );
    let ps_command = format!(
        "$p = Start-Process -FilePath 'cmd.exe' -Verb RunAs -Wait -PassThru -ArgumentList '{}'; if ($p) {{ Write-Output $p.ExitCode }}",
        powershell_single_quote_escape(&arg_list)
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_command])
        .output()
        .map_err(|error| format!("failed to request elevated fsutil run: {error}"))?;

    let exit_code = String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .and_then(|line| line.trim().parse::<i32>().ok())
        .unwrap_or(if output.status.success() { 0 } else { 1 });

    let elevated_stdout = fs::read_to_string(&out_path).unwrap_or_default();
    let elevated_stderr = fs::read_to_string(&err_path).unwrap_or_default();
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_file(&err_path);

    if exit_code == 0 {
        if elevated_stdout.trim().is_empty() {
            Err("elevated fsutil returned no output".to_string())
        } else {
            Ok(elevated_stdout)
        }
    } else {
        let stderr = elevated_stderr.trim();
        let stdout = elevated_stdout.trim();
        let fallback = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr.to_string()
        } else if !stdout.is_empty() {
            stdout.to_string()
        } else if !fallback.is_empty() {
            fallback
        } else {
            "elevated fsutil failed (possibly cancelled at UAC prompt)".to_string()
        };
        Err(message)
    }
}

#[cfg(windows)]
fn powershell_single_quote_escape(value: &str) -> String {
    value.replace('\'', "''")
}

fn is_access_denied_message(message: &str) -> bool {
    message.to_ascii_lowercase().contains("access is denied")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CimVolumeInfo {
    filesystem: String,
    serial: String,
}

#[cfg(windows)]
fn query_cim_volume(drive: &str) -> Result<Option<CimVolumeInfo>, String> {
    let letter = drive.trim_end_matches(':');
    let command = format!(
        "$v = Get-CimInstance Win32_LogicalDisk -Filter \"DeviceID='{}:'\"; if ($v) {{ \"$($v.FileSystem)|$($v.VolumeSerialNumber)\" }}",
        letter
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &command])
        .output()
        .map_err(|error| format!("failed to run PowerShell Get-CimInstance fallback: {error}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    Ok(parse_cim_volume_line(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn normalize_drive(value: &str) -> Result<String, String> {
    let trimmed = value.trim().trim_end_matches('\\');
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        Ok(trimmed.to_ascii_uppercase())
    } else if trimmed.len() == 1 && trimmed.chars().all(|ch| ch.is_ascii_alphabetic()) {
        Ok(format!("{}:", trimmed.to_ascii_uppercase()))
    } else {
        Err("drive must look like H or H:".to_string())
    }
}

fn parse_cim_volume_line(line: &str) -> Option<CimVolumeInfo> {
    let (filesystem, serial) = line.split_once('|')?;
    let filesystem = filesystem.trim();
    let serial = serial.trim();
    if filesystem.is_empty() || serial.is_empty() {
        None
    } else {
        Some(CimVolumeInfo {
            filesystem: filesystem.to_ascii_uppercase(),
            serial: serial.to_ascii_uppercase(),
        })
    }
}

fn parse_ntfs_serial(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("ntfs volume serial number") {
            return None;
        }
        line.split("0x")
            .nth(1)
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| !value.is_empty())
    })
}

fn parse_volume_serial(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("volume serial number") {
            return None;
        }
        line.split(':')
            .nth(1)
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| !value.is_empty())
    })
}

fn print_help() {
    println!("partboot {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Commands:");
    println!("  init --root <path>");
    println!("  scan --root <path> [--json]");
    println!("  extract --root <path> --iso <iso-name-or-path>");
    println!("  generate-menu --root <path> --partition-uuid <uuid> [--partition-label <label>] [--include-diagnostics] [--json] [--output <path>]");
    println!("  stage-efi --root <path> --grub-x64 <path> [--boot-x64 <path>] [--output <path>]");
    println!("  install-esp --root <path> --esp <path> --dry-run");
    println!("  install-esp --root <path> --esp <path> --force");
    println!("  install-fallback --root <path> --esp <path> --dry-run");
    println!("  install-fallback --root <path> --esp <path> --force");
    println!("  boot-instructions --esp <path>");
    println!("  doctor --root <path> [--esp <path>] [--json]");
    println!("  guided-test-flow --root <path> --esp <path> --partition-uuid <uuid> [--partition-label <label>] [--iso <name>] [--include-diagnostics] [--dry-run-install] [--json]");
    println!("  start [--include-diagnostics] [--dry-run-install]  (alias: guided-test-flow-interactive)");
    println!("  volume-id --drive <letter>");
    println!("  recommend-test-partitions");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parse_init_command() {
        let command = parse_command(&args(&["init", "--root", "X:/partboot"])).unwrap();
        assert_eq!(
            command,
            Command::Init {
                root: PathBuf::from("X:/partboot")
            }
        );
    }

    #[test]
    fn parse_scan_command_with_json() {
        let command = parse_command(&args(&["scan", "--root", "H:/partboot", "--json"])).unwrap();
        assert_eq!(
            command,
            Command::Scan {
                root: PathBuf::from("H:/partboot"),
                json: true
            }
        );
    }

    #[test]
    fn parse_generate_menu_command() {
        let command = parse_command(&args(&[
            "generate-menu",
            "--root",
            "X:/partboot",
            "--partition-uuid",
            "ABCD-1234",
            "--output",
            "X:/partboot/generated/grub.cfg",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::GenerateMenu {
                root: PathBuf::from("X:/partboot"),
                partition_uuid: "ABCD-1234".to_string(),
                partition_label: None,
                include_diagnostics: false,
                json: false,
                output: Some(PathBuf::from("X:/partboot/generated/grub.cfg")),
            }
        );
    }

    #[test]
    fn parse_extract_command() {
        let command = parse_command(&args(&[
            "extract",
            "--root",
            "H:/partboot",
            "--iso",
            "ubuntu-22.04.5-desktop-amd64.iso",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::Extract {
                root: PathBuf::from("H:/partboot"),
                iso: "ubuntu-22.04.5-desktop-amd64.iso".to_string()
            }
        );
    }

    #[test]
    fn parse_volume_id_command() {
        let command = parse_command(&args(&["volume-id", "--drive", "H:"])).unwrap();
        assert_eq!(
            command,
            Command::VolumeId {
                drive: "H:".to_string()
            }
        );
    }

    #[test]
    fn parse_stage_efi_command() {
        let command = parse_command(&args(&[
            "stage-efi",
            "--root",
            "X:/partboot",
            "--grub-x64",
            "C:/tmp/grubx64.efi",
            "--output",
            "X:/partboot/efi",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::StageEfi {
                root: PathBuf::from("X:/partboot"),
                grub_x64: PathBuf::from("C:/tmp/grubx64.efi"),
                boot_x64: None,
                output: Some(PathBuf::from("X:/partboot/efi"))
            }
        );
    }

    #[test]
    fn parse_install_esp_command() {
        let command = parse_command(&args(&[
            "install-esp",
            "--root",
            "H:/partboot",
            "--esp",
            "S:/",
            "--dry-run",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::InstallEsp {
                root: PathBuf::from("H:/partboot"),
                esp: PathBuf::from("S:/"),
                dry_run: true,
                force: false
            }
        );
    }

    #[test]
    fn parse_install_fallback_command() {
        let command = parse_command(&args(&[
            "install-fallback",
            "--root",
            "H:/partboot",
            "--esp",
            "S:/",
            "--force",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::InstallFallback {
                root: PathBuf::from("H:/partboot"),
                esp: PathBuf::from("S:/"),
                dry_run: false,
                force: true
            }
        );
    }

    #[test]
    fn parse_boot_instructions_command() {
        let command = parse_command(&args(&["boot-instructions", "--esp", "S:/"])).unwrap();
        assert_eq!(
            command,
            Command::BootInstructions {
                esp: PathBuf::from("S:/")
            }
        );
    }

    #[test]
    fn parse_generate_menu_with_diagnostics_flag() {
        let command = parse_command(&args(&[
            "generate-menu",
            "--root",
            "X:/partboot",
            "--partition-uuid",
            "ABCD-1234",
            "--include-diagnostics",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::GenerateMenu {
                root: PathBuf::from("X:/partboot"),
                partition_uuid: "ABCD-1234".to_string(),
                partition_label: None,
                include_diagnostics: true,
                json: false,
                output: None,
            }
        );
    }

    #[test]
    fn parse_doctor_command_with_esp() {
        let command = parse_command(&args(&["doctor", "--root", "H:/partboot", "--esp", "S:/"]))
            .unwrap();
        assert_eq!(
            command,
            Command::Doctor {
                root: PathBuf::from("H:/partboot"),
                esp: Some(PathBuf::from("S:/")),
                json: false
            }
        );
    }

    #[test]
    fn parse_doctor_command_with_json() {
        let command = parse_command(&args(&["doctor", "--root", "H:/partboot", "--json"])).unwrap();
        assert_eq!(
            command,
            Command::Doctor {
                root: PathBuf::from("H:/partboot"),
                esp: None,
                json: true
            }
        );
    }

    #[test]
    fn parse_guided_test_flow_command() {
        let command = parse_command(&args(&[
            "guided-test-flow",
            "--root",
            "H:/partboot",
            "--esp",
            "S:/",
            "--partition-uuid",
            "9412B8E612B8CF0C",
            "--partition-label",
            "partboottest",
            "--include-diagnostics",
            "--dry-run-install",
            "--json",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::GuidedTestFlow {
                root: PathBuf::from("H:/partboot"),
                esp: PathBuf::from("S:/"),
                partition_uuid: "9412B8E612B8CF0C".to_string(),
                partition_label: Some("partboottest".to_string()),
                iso: None,
                include_diagnostics: true,
                json: true,
                dry_run_install: true
            }
        );
    }

    #[test]
    fn parse_guided_test_flow_interactive_command() {
        let command = parse_command(&args(&[
            "guided-test-flow-interactive",
            "--include-diagnostics",
            "--dry-run-install",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::GuidedTestFlowInteractive {
                include_diagnostics: true,
                dry_run_install: true
            }
        );
    }

    #[test]
    fn parse_start_alias_command() {
        let command = parse_command(&args(&["start", "--include-diagnostics", "--dry-run-install"]))
            .unwrap();
        assert_eq!(
            command,
            Command::GuidedTestFlowInteractive {
                include_diagnostics: true,
                dry_run_install: true
            }
        );
    }

    #[test]
    fn drive_from_path_extracts_windows_drive() {
        assert_eq!(drive_from_path(&PathBuf::from("s:/")).unwrap(), "S:");
        assert_eq!(drive_from_path(&PathBuf::from("S:/EFI")).unwrap(), "S:");
        assert!(drive_from_path(&PathBuf::from("/mnt/esp")).is_err());
    }

    #[test]
    fn parse_ntfs_serial_from_fsutil_output() {
        let output = "NTFS Volume Serial Number :        0x9cfe412afe41021d";
        assert_eq!(
            parse_ntfs_serial(output),
            Some("9CFE412AFE41021D".to_string())
        );
    }

    #[test]
    fn normalize_drive_letter() {
        assert_eq!(normalize_drive("h").unwrap(), "H:");
        assert_eq!(normalize_drive("H:").unwrap(), "H:");
        assert!(normalize_drive("H:/partboot").is_err());
    }

    #[test]
    fn parse_cim_volume_line_splits_filesystem_and_serial() {
        assert_eq!(
            parse_cim_volume_line("NTFS|12B8CF0C"),
            Some(CimVolumeInfo {
                filesystem: "NTFS".to_string(),
                serial: "12B8CF0C".to_string()
            })
        );
        assert_eq!(parse_cim_volume_line("NTFS|"), None);
    }

    #[test]
    fn full_hex_uuid_check_requires_long_hex() {
        assert!(is_full_hex_uuid("9412B8E612B8CF0C"));
        assert!(is_full_hex_uuid("9412-B8E6-12B8-CF0C"));
        assert!(!is_full_hex_uuid("12B8CF0C"));
        assert!(!is_full_hex_uuid("XYZ-1234"));
    }

    #[test]
    fn parse_checksum_manifest_accepts_basic_format() {
        let manifest = "\
            # bundled efi checksums\n\
            grubx64.efi=DEADBEEF\n\
            bootx64.efi=CAFEBABE\n";
        let entries = parse_checksum_manifest(manifest).unwrap();
        assert_eq!(
            entries,
            vec![
                ("grubx64.efi".to_string(), "DEADBEEF".to_string()),
                ("bootx64.efi".to_string(), "CAFEBABE".to_string())
            ]
        );
    }

    #[test]
    fn crc32_matches_known_value() {
        assert_eq!(format!("{:08X}", crc32(b"123456789")), "CBF43926");
    }

    #[test]
    fn import_iso_from_drive_root_moves_file_when_possible() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("partboot-import-test-{unique}"));
        let source_dir = base.join("source");
        let destination_dir = base.join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&destination_dir).unwrap();

        let source = source_dir.join("sample.iso");
        let destination = destination_dir.join("sample.iso");
        std::fs::write(&source, b"iso-bytes").unwrap();

        let mode = import_iso_from_drive_root(&source, &destination).unwrap();
        assert_eq!(mode, ImportMode::Moved);
        assert!(!source.exists());
        assert!(destination.exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn json_escape_quotes_and_backslashes() {
        assert_eq!(
            json_escape("H:\\partboot\\\"test\""),
            "H:\\\\partboot\\\\\\\"test\\\""
        );
    }

    #[test]
    fn detect_access_denied_message() {
        assert!(is_access_denied_message("Error 5: Access is denied."));
        assert!(!is_access_denied_message("file not found"));
    }
}
