mod boot_entry;
mod cache;
mod extract;
mod grub;
mod iso;
mod layout;
mod profile;
mod spinner;

use crate::boot_entry::{
    create_boot_entry, find_partboot_entries_for_loader, find_stale_partboot_entries,
    list_firmware_entries, remove_boot_entry, restore_boot_entries,
};
use crate::extract::{
    extract_casper, is_complete_extracted, is_supported_linux_family, mark_extracted_images,
};
use crate::grub::generate_grub_cfg;
use crate::iso::{scan_iso_dir, support_label};
use crate::layout::PartBootLayout;
use crate::profile::{
    count_profile_files, ensure_profile_for_iso_name, ensure_profiles_for_images,
    load_profiles_for_images,
};
use clap::{Parser, Subcommand};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use std::thread;
use std::time::Duration;

fn partboot_styles() -> clap::builder::Styles {
    use clap::builder::styling::AnsiColor;
    clap::builder::Styles::styled()
        .header(AnsiColor::Green.on_default().bold())
        .literal(AnsiColor::Cyan.on_default().bold())
        .usage(AnsiColor::Green.on_default().bold())
}

#[derive(Parser)]
#[command(
    name = "partboot",
    bin_name = "partboot",
    version,
    about = "Disk-resident ISO boot manager",
    long_about = "\
PartBoot is a disk-resident ISO boot manager for UEFI systems. It lets you
keep Linux ISO images on a local partition and boot them through a generated
GRUB menu instead of preparing a USB drive for each installer or live image.",
    max_term_width = 120,
    color = clap::ColorChoice::Always,
    styles = partboot_styles(),
    after_help = "See \u{1b}[1m\u{1b}[36m'partboot help <command>'\u{1b}[0m for more information on a specific command.",
    help_template = "{about}\n\n\u{1b}[1m\u{1b}[32mUsage:\u{1b}[0m\n  {usage}\n\n\u{1b}[1m\u{1b}[32mCommands:\u{1b}[0m\n{subcommands}\n\n\u{1b}[1m\u{1b}[32mOptions:\u{1b}[0m\n{options}\n{after-help}",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Output results as JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
enum Command {
    /// Initialize a PartBoot root directory
    Init {
        /// Path to the PartBoot root directory
        #[arg(long)]
        root: PathBuf,
    },
    /// Scan the ISO directory and detect supported images
    Scan {
        /// Path to the PartBoot root directory
        #[arg(long)]
        root: PathBuf,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate GRUB configuration for discovered ISOs
    Menu {
        /// Path to the PartBoot root directory
        #[arg(long)]
        root: PathBuf,
        /// NTFS partition UUID (short hex or full)
        #[arg(long = "uuid")]
        partition_uuid: String,
        /// NTFS partition label (optional, improves GRUB menu display)
        #[arg(long = "label")]
        partition_label: Option<String>,
        /// Include a diagnostics boot entry
        #[arg(long = "diagnostics")]
        include_diagnostics: bool,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
        /// Write output to a specific file path
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Extract boot files from an ISO image
    Extract {
        /// Path to the PartBoot root directory
        #[arg(long)]
        root: PathBuf,
        /// ISO file name (in isos/) or full path to the ISO
        #[arg(long)]
        iso: String,
    },
    /// Stage EFI binaries into the PartBoot efi/ directory
    Stage {
        /// Path to the PartBoot root directory
        #[arg(long)]
        root: PathBuf,
        /// Path to grubx64.efi
        #[arg(long)]
        grub_x64: PathBuf,
        /// Path to bootx64.efi (optional)
        #[arg(long)]
        boot_x64: Option<PathBuf>,
        /// Output to a specific directory instead of the default
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Install PartBoot EFI files to the ESP
    Esp {
        /// Path to the PartBoot root directory
        #[arg(long)]
        root: PathBuf,
        /// Path to the EFI System Partition
        #[arg(long)]
        esp: PathBuf,
        /// Preview changes without modifying files
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Force overwrite existing files
        #[arg(long = "force")]
        force: bool,
    },
    /// Install PartBoot as UEFI fallback boot option
    Fallback {
        /// Path to the PartBoot root directory
        #[arg(long)]
        root: PathBuf,
        /// Path to the EFI System Partition
        #[arg(long)]
        esp: PathBuf,
        /// Preview changes without modifying files
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Force overwrite existing files
        #[arg(long)]
        force: bool,
    },
    /// Show manual boot instructions for the ESP
    Boot {
        /// Path to the EFI System Partition
        #[arg(long)]
        esp: PathBuf,
    },
    /// Run health checks on the PartBoot installation
    Doctor {
        /// Path to the PartBoot root directory
        #[arg(long)]
        root: PathBuf,
        /// Path to the EFI System Partition (optional)
        #[arg(long)]
        esp: Option<PathBuf>,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
    /// Set up PartBoot interactively or with explicit parameters
    Start {
        /// Path to the PartBoot root directory (scripted mode)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Path to the EFI System Partition (scripted mode)
        #[arg(long)]
        esp: Option<PathBuf>,
        /// NTFS partition UUID (required with --root and --esp)
        #[arg(long = "uuid")]
        partition_uuid: Option<String>,
        /// NTFS partition label
        #[arg(long = "label")]
        partition_label: Option<String>,
        /// Extract only this ISO file name
        #[arg(long)]
        iso: Option<String>,
        /// Include a diagnostics boot entry
        #[arg(long = "diagnostics")]
        include_diagnostics: bool,
        /// Preview install without modifying files
        #[arg(long = "dry-run")]
        dry_run_install: bool,
        /// Skip firmware boot entry creation
        #[arg(long = "skip-entry")]
        skip_boot_entry: bool,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
    /// Display the NTFS volume ID for a drive letter
    Vol {
        /// Windows drive letter (e.g. H or H:)
        #[arg(long)]
        drive: String,
    },
    /// Print safe test-partition guidance
    Test,
    /// Manage UEFI firmware boot entries
    #[command(name = "entry", subcommand)]
    BootEntry(BootEntryCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
enum BootEntryCommand {
    /// List firmware boot entries
    List {
        /// Show only PartBoot-managed entries
        #[arg(long = "partboot")]
        partboot_only: bool,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
    /// Create a new UEFI firmware boot entry
    Create {
        /// Path to the EFI System Partition
        #[arg(long)]
        esp: PathBuf,
        /// PartBoot root directory (auto-resolves loader)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Human-readable entry name
        #[arg(long)]
        label: String,
        /// Explicit ESP-relative loader path (alternative to --root)
        #[arg(long)]
        loader: Option<String>,
        /// Add entry to the top of the boot order
        #[arg(long = "first")]
        first: bool,
        /// Validate inputs without modifying firmware
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a firmware boot entry by its GUID
    Remove {
        /// Boot entry identifier (e.g. {xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx})
        #[arg(long)]
        id: String,
        /// Validate inputs without modifying firmware
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
    /// Restore a previously exported BCD backup
    Restore {
        /// Path to the BCD backup file
        #[arg(long)]
        backup: PathBuf,
        /// Validate inputs without modifying firmware
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Start {
        root: None,
        esp: None,
        partition_uuid: None,
        partition_label: None,
        iso: None,
        include_diagnostics: false,
        dry_run_install: false,
        skip_boot_entry: false,
        json: false,
    });

    if let Err(error) = run(command) {
        eprintln!("error: {error}");
        process::exit(2);
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
        Command::Menu {
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
                "extracted Linux boot files to {}",
                layout.extracted.join(extracted_id).display()
            );
        }
        Command::Stage {
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
        Command::Esp {
            root,
            esp,
            dry_run,
            force,
        } => {
            install_esp(&PartBootLayout::new(root), &esp, dry_run, force)?;
        }
        Command::Fallback {
            root,
            esp,
            dry_run,
            force,
        } => {
            install_fallback(&PartBootLayout::new(root), &esp, dry_run, force)?;
        }
        Command::Boot { esp } => {
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
        Command::Start {
            root,
            esp,
            partition_uuid,
            partition_label,
            iso,
            include_diagnostics,
            dry_run_install,
            skip_boot_entry,
            json,
        } => {
            if let (Some(root), Some(esp), Some(partition_uuid)) = (root, esp, partition_uuid) {
                run_start_scripted(
                    root,
                    esp,
                    partition_uuid,
                    partition_label,
                    iso,
                    include_diagnostics,
                    json,
                    dry_run_install,
                )?;
            } else {
                run_start_interactive(include_diagnostics, dry_run_install, skip_boot_entry, json)?;
            }
        }
        Command::Vol { drive } => {
            print_volume_id(&drive)?;
        }
        Command::Test => {
            print_partition_recommendation();
        }
        Command::BootEntry(sub) => match sub {
            BootEntryCommand::List {
                partboot_only,
                json,
            } => {
                let entries = list_firmware_entries(partboot_only)?;
                if json {
                    let items: Vec<String> = entries
                        .iter()
                        .map(|entry| {
                            format!(
                                "{{\"kind\":\"{}\",\"id\":\"{}\",\"description\":\"{}\",\"device\":\"{}\",\"path\":\"{}\",\"order\":{}}}",
                                json_escape(&entry.kind),
                                json_escape(&entry.identifier),
                                json_escape(entry.description.as_deref().unwrap_or("")),
                                json_escape(entry.device.as_deref().unwrap_or("")),
                                json_escape(entry.path.as_deref().unwrap_or("")),
                                entry
                                    .display_order_index
                                    .map(|value| value.to_string())
                                    .unwrap_or_else(|| "null".to_string())
                            )
                        })
                        .collect();
                    println!("{{\"entries\":[{}]}}", items.join(","));
                } else if entries.is_empty() {
                    println!("[ok] no firmware application entries found");
                } else {
                    println!("[ok] firmware boot entries:");
                    for entry in entries {
                        println!(
                            "- {} | {} | {} | {} | order={}",
                            entry.identifier,
                            entry
                                .description
                                .unwrap_or_else(|| "(no description)".to_string()),
                            entry.path.unwrap_or_else(|| "(no path)".to_string()),
                            entry.kind,
                            entry
                                .display_order_index
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| "-".to_string())
                        );
                    }
                }
            }
            BootEntryCommand::Create {
                esp,
                root,
                label,
                loader,
                first,
                dry_run,
                json,
            } => {
                let result = create_boot_entry(
                    &esp,
                    root.as_deref(),
                    &label,
                    loader.as_deref(),
                    first,
                    dry_run,
                )?;
                if json {
                    println!(
                        "{{\"id\":\"{}\",\"label\":\"{}\",\"loader\":\"{}\",\"dry_run\":{},\"first\":{},\"reused\":{},\"secure_boot\":{},\"backup\":\"{}\"}}",
                        json_escape(&result.identifier),
                        json_escape(&result.label),
                        json_escape(&result.loader),
                        if result.dry_run { "true" } else { "false" },
                        if result.added_first { "true" } else { "false" },
                        if result.reused_existing { "true" } else { "false" },
                        match result.secure_boot_enabled {
                            Some(true) => "true",
                            Some(false) => "false",
                            None => "null",
                        },
                        json_escape(
                            &result
                                .backup_path
                                .as_ref()
                                .map(|path| path.to_string_lossy().to_string())
                                .unwrap_or_default()
                        )
                    );
                } else if result.dry_run {
                    println!("[ok] dry-run only; no firmware changes applied");
                    println!("label: {}", result.label);
                    println!("loader: {}", result.loader);
                    println!("placement: {}", if first { "first" } else { "last" });
                    println!(
                        "secure boot: {}",
                        match result.secure_boot_enabled {
                            Some(true) => "enabled",
                            Some(false) => "disabled",
                            None => "unknown",
                        }
                    );
                } else {
                    if result.reused_existing {
                        println!("[ok] reused existing firmware entry {}", result.identifier);
                    } else {
                        println!("[ok] created firmware entry {}", result.identifier);
                    }
                    println!("label: {}", result.label);
                    println!("loader: {}", result.loader);
                    println!("placement: {}", if first { "first" } else { "last" });
                    println!(
                        "secure boot: {}",
                        match result.secure_boot_enabled {
                            Some(true) => "enabled",
                            Some(false) => "disabled",
                            None => "unknown",
                        }
                    );
                    if let Some(path) = result.backup_path {
                        println!("backup: {}", path.display());
                        println!("restore: bcdedit /import {}", path.display());
                    }
                }
            }
            BootEntryCommand::Remove { id, dry_run, json } => {
                let result = remove_boot_entry(&id, dry_run)?;
                if json {
                    println!(
                        "{{\"id\":\"{}\",\"dry_run\":{},\"secure_boot\":{},\"backup\":\"{}\"}}",
                        json_escape(&result.identifier),
                        if result.dry_run { "true" } else { "false" },
                        match result.secure_boot_enabled {
                            Some(true) => "true",
                            Some(false) => "false",
                            None => "null",
                        },
                        json_escape(
                            &result
                                .backup_path
                                .as_ref()
                                .map(|path| path.to_string_lossy().to_string())
                                .unwrap_or_default()
                        )
                    );
                } else if result.dry_run {
                    println!("[ok] dry-run only; entry not removed");
                    println!("id: {}", result.identifier);
                    println!(
                        "secure boot: {}",
                        match result.secure_boot_enabled {
                            Some(true) => "enabled",
                            Some(false) => "disabled",
                            None => "unknown",
                        }
                    );
                } else {
                    println!("[ok] removed firmware entry {}", result.identifier);
                    println!(
                        "secure boot: {}",
                        match result.secure_boot_enabled {
                            Some(true) => "enabled",
                            Some(false) => "disabled",
                            None => "unknown",
                        }
                    );
                    if let Some(path) = result.backup_path {
                        println!("backup: {}", path.display());
                        println!("restore: bcdedit /import {}", path.display());
                    }
                }
            }
            BootEntryCommand::Restore {
                backup,
                dry_run,
                json,
            } => {
                let result = restore_boot_entries(&backup, dry_run)?;
                if json {
                    println!(
                        "{{\"backup\":\"{}\",\"dry_run\":{},\"secure_boot\":{}}}",
                        json_escape(&result.backup_path.to_string_lossy()),
                        if result.dry_run { "true" } else { "false" },
                        match result.secure_boot_enabled {
                            Some(true) => "true",
                            Some(false) => "false",
                            None => "null",
                        }
                    );
                } else if result.dry_run {
                    println!("[ok] dry-run only; backup not imported");
                    println!("backup: {}", result.backup_path.display());
                } else {
                    println!(
                        "[ok] restored BCD store from {}",
                        result.backup_path.display()
                    );
                }
            }
        },
    }
    Ok(())
}

const PARTBOOT_ASCII: &str = r" ____   __   ____  ____  ____   __    __  ____ 
(  _ \ / _\ (  _ \(_  _)(  _ \ /  \  /  \(_  _)
 ) __//    \ )   /  )(   ) _ ((  O )(  O ) )(  
(__)  \_/\_/(__\_) (__) (____/ \__/  \__/ (__)";

#[derive(Debug, Default)]
struct UiRuntimeState {
    fullscreen: bool,
    lines: Vec<String>,
}

static UI_RUNTIME: OnceLock<Mutex<UiRuntimeState>> = OnceLock::new();

fn ui_runtime() -> &'static Mutex<UiRuntimeState> {
    UI_RUNTIME.get_or_init(|| Mutex::new(UiRuntimeState::default()))
}

fn ui_is_fullscreen() -> bool {
    ui_runtime()
        .lock()
        .map(|state| state.fullscreen)
        .unwrap_or(false)
}

fn ui_emit_line(line: String) {
    let mut state = match ui_runtime().lock() {
        Ok(state) => state,
        Err(_) => {
            println!("{line}");
            return;
        }
    };

    if !state.fullscreen {
        println!("{line}");
        return;
    }

    state.lines.push(line);
    ui_render_fullscreen_locked(&state);
}

fn ui_render_fullscreen_locked(state: &UiRuntimeState) {
    use crossterm::cursor::MoveTo;
    use crossterm::execute;
    use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
    use crossterm::terminal::{Clear, ClearType};

    let mut stdout = io::stdout();
    if execute!(
        stdout,
        MoveTo(0, 0),
        Clear(ClearType::All),
        SetForegroundColor(Color::Cyan),
        SetAttribute(Attribute::Bold),
        Print(format!("{PARTBOOT_ASCII}\n")),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Print("====================\n"),
        SetForegroundColor(Color::DarkGrey),
        Print("Running guided flow...\n\n"),
        ResetColor
    )
    .is_err()
    {
        return;
    }

    // Keep only the latest lines so the footer remains visible on smaller terminals.
    let max_lines = 26usize;
    let start = state.lines.len().saturating_sub(max_lines);
    for line in &state.lines[start..] {
        let _ = execute!(stdout, Print(line), Print("\n"));
    }
    let _ = stdout.flush();
}

fn ui_run_fullscreen<T, F>(operation: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    use crossterm::cursor::{Hide, Show};
    use crossterm::execute;
    use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};

    struct FullscreenGuard;
    impl Drop for FullscreenGuard {
        fn drop(&mut self) {
            if let Ok(mut state) = ui_runtime().lock() {
                state.fullscreen = false;
                state.lines.clear();
            }
            let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        }
    }

    execute!(io::stdout(), EnterAlternateScreen, Hide)
        .map_err(|error| format!("failed to start fullscreen UI: {error}"))?;
    if let Ok(mut state) = ui_runtime().lock() {
        state.fullscreen = true;
        state.lines.clear();
    }
    let _guard = FullscreenGuard;
    operation()
}

fn ui_section(title: &str) {
    ui_emit_line(String::new());
    ui_emit_line(format!("== {title} =="));
}

fn ui_ok(message: &str) {
    ui_emit_line(format!("[ok] {message}"));
}

fn ui_warn(message: &str) {
    ui_emit_line(format!("[warn] {message}"));
}

fn ui_kv(label: &str, value: &str) {
    ui_emit_line(format!("  {:<26} {}", format!("{label}:"), value));
}

fn status(path: &Path) -> &'static str {
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

#[allow(clippy::too_many_arguments)]
fn run_start_scripted(
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
        ui_section("Initialize");
        ui_ok(&format!("Initialized {}", layout.root.display()));
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
            ui_section("Import");
            ui_ok(&format!(
                "Imported {} ISO file(s) from drive root into {}",
                imported_drive_isos.len(),
                layout.isos.display()
            ));
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
        ui_section("Scan");
        ui_ok(&format!("Found {} ISO image(s)", images.len()));
    }

    if !json {
        ui_section("Extract");
    }

    let selected_isos: Vec<String> = if let Some(iso_name) = iso {
        vec![iso_name]
    } else {
        images
            .iter()
            .filter(|image| is_supported_linux_family(&image.family))
            .map(|image| image.name.clone())
            .collect()
    };

    let mut extracted_targets = Vec::new();
    let mut extract_failures = Vec::new();
    if selected_isos.is_empty() {
        if !json {
            ui_warn("No supported Linux ISO found; skipping extract step");
        }
    } else {
        for iso_name in &selected_isos {
            let extract_label = format!("extract {}", iso_name);
            let extraction_result =
                run_with_spinner(!json && !ui_is_fullscreen(), &extract_label, || {
                    extract_casper(&layout, iso_name)?;
                    ensure_profile_for_iso_name(&layout, iso_name)?;
                    Ok(())
                });
            match extraction_result {
                Ok(()) => {
                    if !json {
                        ui_ok(&format!("Extracted {}", iso_name));
                    }
                    extracted_targets.push(iso_name.clone());
                }
                Err(error) => {
                    if !json {
                        ui_warn(&format!("Extract step skipped for {}: {}", iso_name, error));
                    }
                    extract_failures.push(iso_name.clone());
                }
            }
        }
        if !json {
            ui_kv("Extraction requested", &selected_isos.len().to_string());
            ui_kv("Extraction succeeded", &extracted_targets.len().to_string());
            ui_kv("Extraction skipped", &extract_failures.len().to_string());
        }
    }

    images = scan_iso_dir(&layout.isos).map_err(|error| error.to_string())?;
    mark_extracted_images(&layout, &mut images);
    let profiles = load_profiles_for_images(&layout, &images)?;
    let generated_cfg = layout.grub_cfg_path();
    run_with_spinner(!json && !ui_is_fullscreen(), "menu", || {
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
        ui_ok(&format!("Wrote {}", generated_cfg.display()));
    }

    let efi_binaries = resolve_efi_binaries_for_stage(&layout)?;
    if !json && efi_binaries.copied_from_bundle {
        ui_section("Cache");
        ui_ok(&format!(
            "Populated cache binaries from {}",
            efi_binaries.source
        ));
    }
    let staged = stage_efi(
        &layout,
        &efi_binaries.grub_x64,
        Some(&efi_binaries.boot_x64),
        None,
    )?;
    if !json {
        ui_section("Stage EFI");
        ui_ok(&format!("Staged {}", staged.display()));
    }

    install_esp(&layout, &esp, dry_run_install, !dry_run_install)?;
    install_fallback(&layout, &esp, dry_run_install, !dry_run_install)?;
    if !json {
        ui_section("Install");
        if dry_run_install {
            ui_ok("Install steps executed in dry-run mode");
        } else {
            ui_ok(&format!("Installed to {}", esp.display()));
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
            "{{\"root\":\"{}\",\"esp\":\"{}\",\"partition_uuid\":\"{}\",\"partition_label\":\"{}\",\"image_count\":{},\"imported_drive_root_isos\":{},\"created_profiles\":[{}],\"extracted_iso\":\"{}\",\"extracted_isos\":[{}],\"generated_cfg\":\"{}\",\"staged_dir\":\"{}\",\"efi_binary_source\":\"{}\",\"dry_run_install\":{},\"doctor\":{{\"full_ntfs_uuid_present\":\"{}\",\"extracted_files_complete\":\"{}\",\"profiles_present\":\"{}\",\"esp_files_installed\":\"{}\",\"fallback_installed\":\"{}\"}}}}",
            json_escape(&layout.root.to_string_lossy()),
            json_escape(&esp.to_string_lossy()),
            json_escape(&partition_uuid),
            json_escape(partition_label.as_deref().unwrap_or("")),
            images.len(),
            imported_drive_isos.len(),
            created_items.join(","),
            json_escape(extracted_targets.first().map(String::as_str).unwrap_or("")),
            extracted_targets
                .iter()
                .map(|name| format!("\"{}\"", json_escape(name)))
                .collect::<Vec<_>>()
                .join(","),
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
        ui_section("Health Check");
        ui_kv("Full NTFS UUID present", &ntfs_uuid_status);
        ui_kv("Extracted files complete", &extracted_status);
        ui_kv("Profiles present", &profiles_status);
        ui_kv("ESP files installed", &esp_status);
        ui_kv("Fallback installed", &fallback_status);
        ui_section("Summary");
        ui_kv("Root", &layout.root.display().to_string());
        ui_kv("ESP", &esp.display().to_string());
        ui_kv("ISO count", &images.len().to_string());
        let extracted_iso_summary = if extracted_targets.is_empty() {
            "(none)".to_string()
        } else {
            extracted_targets.join(", ")
        };
        ui_kv("Extracted ISO(s)", &extracted_iso_summary);
        ui_kv("Generated config", &generated_cfg.display().to_string());
        ui_kv("Staged EFI dir", &staged.display().to_string());
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
    let _ = writeln!(io::stdout());
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

fn download_release_efi_assets(
    layout: &PartBootLayout,
) -> Result<Option<DownloadedEfiAssets>, String> {
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
        return Ok(());
    }

    let ps_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let ps_stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let ps_details = if !ps_stderr.is_empty() {
        ps_stderr
    } else if !ps_stdout.is_empty() {
        ps_stdout
    } else {
        "unknown PowerShell download failure".to_string()
    };

    let destination_owned = destination.to_string_lossy().to_string();
    let curl_output = process::Command::new("curl.exe")
        .args([
            "--location",
            "--fail",
            "--silent",
            "--show-error",
            "--output",
            &destination_owned,
            url,
        ])
        .output();

    match curl_output {
        Ok(curl) if curl.status.success() => Ok(()),
        Ok(curl) => {
            let curl_stderr = String::from_utf8_lossy(&curl.stderr).trim().to_string();
            let curl_stdout = String::from_utf8_lossy(&curl.stdout).trim().to_string();
            let curl_details = if !curl_stderr.is_empty() {
                curl_stderr
            } else if !curl_stdout.is_empty() {
                curl_stdout
            } else {
                "unknown curl download failure".to_string()
            };
            Err(format!(
                "failed downloading {}: PowerShell error: {}; curl.exe error: {}",
                url, ps_details, curl_details
            ))
        }
        Err(error) => Err(format!(
            "failed downloading {}: PowerShell error: {}; curl.exe unavailable: {}",
            url, ps_details, error
        )),
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
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {}", path.display(), error))?;
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

fn run_start_interactive(
    include_diagnostics: bool,
    dry_run_install: bool,
    skip_boot_entry: bool,
    _json: bool,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = include_diagnostics;
        let _ = dry_run_install;
        let _ = skip_boot_entry;
        let _ = json;
        return Err("start is currently Windows only".to_string());
    }

    #[cfg(windows)]
    {
        let spinner = crate::spinner::Spinner::new("Scanning partitions");
        let volumes = list_windows_volumes()?;
        spinner.finish("Partition scan complete");

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

        ui_run_fullscreen(move || {
            let root_choice = choose_volume_tui(
                "Select NTFS partition for PartBoot root",
                "Use Up/Down arrows (or j/k), Enter confirms",
                &root_candidates,
            )?;
            let esp_choice = choose_volume_tui(
                "Select FAT32 partition for ESP",
                "Use Up/Down arrows (or j/k), Enter confirms",
                &esp_candidates,
            )?;

            ui_section("Preparing");
            ui_ok("Selections confirmed");
            ui_kv("Selected root drive", &root_choice.drive);
            ui_kv("Selected ESP drive", &esp_choice.drive);
            ui_ok("Detecting partition UUID...");

            let root = PathBuf::from(format!("{}\\partboot", root_choice.drive));
            let esp = PathBuf::from(format!("{}\\", esp_choice.drive));
            let partition_uuid = detect_partition_uuid(&root_choice.drive)?;
            let partition_label = root_choice.label.clone();
            ui_ok("Partition UUID detected");

            ui_section("Selected configuration");
            ui_kv("Root", &root.display().to_string());
            ui_kv("ESP", &esp.display().to_string());
            ui_kv("Partition UUID", &partition_uuid);
            ui_kv(
                "Partition label",
                partition_label.as_deref().unwrap_or("(none)"),
            );
            ui_kv(
                "Install mode",
                if dry_run_install {
                    "dry-run"
                } else {
                    "write changes"
                },
            );
            let boot_entry_status = if skip_boot_entry {
                "skip"
            } else if dry_run_install {
                "skipped (dry-run)"
            } else {
                #[cfg(windows)]
                {
                    if check_admin_status() {
                        "create after install (elevated)"
                    } else {
                        "skipped (run as Admin to enable)"
                    }
                }
                #[cfg(not(windows))]
                {
                    "n/a"
                }
            };
            ui_kv("Firmware boot entry", boot_entry_status);

            if !confirm_run_plan(dry_run_install)? {
                ui_warn("Cancelled before execution.");
                return Ok(());
            }

            let result = run_start_scripted(
                root.clone(),
                esp.clone(),
                partition_uuid,
                partition_label,
                None,
                include_diagnostics,
                false,
                dry_run_install,
            );
            if let Err(error) = &result {
                ui_warn(&format!("Guided flow failed: {error}"));
                wait_for_exit_acknowledgement()?;
                return result;
            }

            if !skip_boot_entry && !dry_run_install {
                match offer_boot_entry_creation(&esp, &root) {
                    Ok(()) => {}
                    Err(error) => {
                        ui_warn(&format!("Boot entry step skipped: {error}"));
                    }
                }
            } else if skip_boot_entry {
                ui_section("Boot Entry");
                ui_warn("Firmware boot entry creation skipped (--skip)");
                ui_kv(
                    "Manual alternative",
                    "partboot boot-entry create --esp <path> --root <path> --label PartBoot",
                );
            } else {
                ui_section("Boot Entry");
                ui_warn("Firmware boot entry creation skipped (dry-run mode)");
                ui_kv(
                    "After install",
                    "Run: partboot boot-entry create --esp <path> --root <path> --label PartBoot",
                );
            }

            wait_for_exit_acknowledgement()?;
            result
        })
    }
}

#[cfg(windows)]
fn choose_volume_tui(
    title: &str,
    hint: &str,
    volumes: &[WindowsVolume],
) -> Result<WindowsVolume, String> {
    use crossterm::cursor::{Hide, MoveTo, Show};
    use crossterm::event::{read, Event, KeyCode, KeyEventKind};
    use crossterm::execute;
    use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
    use crossterm::terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    };

    struct TuiGuard {
        owns_terminal: bool,
    }
    impl Drop for TuiGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            if self.owns_terminal {
                let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
            }
        }
    }

    if volumes.is_empty() {
        return Err("no volumes available for selection".to_string());
    }

    let owns_terminal = !ui_is_fullscreen();
    enable_raw_mode().map_err(|error| format!("failed to enable raw mode: {error}"))?;
    if owns_terminal {
        execute!(io::stdout(), EnterAlternateScreen, Hide)
            .map_err(|error| format!("failed to initialize terminal UI: {error}"))?;
    }
    let _guard = TuiGuard { owns_terminal };

    let mut selected = 0usize;
    loop {
        execute!(
            io::stdout(),
            MoveTo(0, 0),
            Clear(ClearType::All),
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold),
            Print(format!("{PARTBOOT_ASCII}\n")),
            SetAttribute(Attribute::Reset),
            ResetColor,
            Print("====================\n"),
            Print(format!("{title}\n")),
            SetForegroundColor(Color::DarkGrey),
            Print(format!("{hint}\n\n")),
            ResetColor
        )
        .map_err(|error| format!("failed to draw terminal UI: {error}"))?;

        for (index, volume) in volumes.iter().enumerate() {
            let label = volume.label.as_deref().unwrap_or("(no-label)");
            if index == selected {
                execute!(
                    io::stdout(),
                    SetForegroundColor(Color::Green),
                    SetAttribute(Attribute::Bold),
                    Print(format!(
                        "> {:<4} {:<6} {}\n",
                        volume.drive, volume.filesystem, label
                    )),
                    SetAttribute(Attribute::Reset),
                    ResetColor
                )
                .map_err(|error| format!("failed to draw selection row: {error}"))?;
            } else {
                execute!(
                    io::stdout(),
                    Print(format!(
                        "  {:<4} {:<6} {}\n",
                        volume.drive, volume.filesystem, label
                    ))
                )
                .map_err(|error| format!("failed to draw row: {error}"))?;
            }
        }

        let current = &volumes[selected];
        let current_label = current.label.as_deref().unwrap_or("(no-label)");
        execute!(
            io::stdout(),
            Print("\n"),
            SetForegroundColor(Color::DarkGrey),
            Print("Selected\n"),
            ResetColor,
            Print(format!("  Drive:      {}\n", current.drive)),
            Print(format!("  Filesystem: {}\n", current.filesystem)),
            Print(format!("  Label:      {}\n", current_label)),
            Print("\n"),
            SetForegroundColor(Color::DarkGrey),
            Print("Controls: Up/Down or j/k, Enter confirm, q/Esc cancel\n"),
            ResetColor
        )
        .map_err(|error| format!("failed to draw footer: {error}"))?;
        io::stdout().flush().map_err(|error| error.to_string())?;

        if let Event::Key(key_event) =
            read().map_err(|error| format!("failed reading key: {error}"))?
        {
            if key_event.kind != KeyEventKind::Press {
                continue;
            }
            match key_event.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = if selected == 0 {
                        volumes.len() - 1
                    } else {
                        selected - 1
                    };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1) % volumes.len();
                }
                KeyCode::Enter => return Ok(volumes[selected].clone()),
                KeyCode::Esc | KeyCode::Char('q') => return Err("selection cancelled".to_string()),
                KeyCode::Char(ch) if ch.is_ascii_digit() => {
                    if let Some(digit) = ch.to_digit(10) {
                        let index = digit as usize;
                        if index >= 1 && index <= volumes.len() {
                            selected = index - 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn confirm_run_plan(dry_run_install: bool) -> Result<bool, String> {
    if dry_run_install {
        print!("Proceed with dry-run execution? [y/N]: ");
    } else {
        print!("Proceed with write/install actions on selected drives? [y/N]: ");
    }
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| error.to_string())?;
    let entered = input.trim().to_ascii_lowercase();
    Ok(matches!(entered.as_str(), "y" | "yes"))
}

fn wait_for_exit_acknowledgement() -> Result<(), String> {
    print!("Press Enter to exit...");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn offer_boot_entry_creation(esp: &Path, root: &Path) -> Result<(), String> {
    ui_section("Firmware Boot Entry");

    let secure_boot = crate::boot_entry::secure_boot_state();
    let secure_boot_label = match secure_boot {
        Some(true) => "enabled (may block unsigned EFI binaries)",
        Some(false) => "disabled",
        None => "unknown",
    };
    ui_kv("Secure Boot", secure_boot_label);

    let loader = "\\EFI\\PartBoot\\grubx64.efi";
    let label = "PartBoot";
    let matching = find_partboot_entries_for_loader(loader)?;
    let stale = find_stale_partboot_entries(loader, label)?;

    if !matching.is_empty() {
        ui_kv(
            "Existing entry",
            &format!(
                "{} entries found pointing to the same loader",
                matching.len()
            ),
        );
        for entry in &matching {
            ui_kv(
                "  Entry",
                &format!(
                    "{} ({})",
                    entry.identifier,
                    entry.description.as_deref().unwrap_or("no description")
                ),
            );
        }
    }
    if !stale.is_empty() {
        ui_kv(
            "Stale entries",
            &format!("{} entries found for different ESP/loader", stale.len()),
        );
        for entry in &stale {
            ui_kv(
                "  Stale",
                &format!(
                    "{} ({}) -> {}",
                    entry.identifier,
                    entry.description.as_deref().unwrap_or("no description"),
                    entry.path.as_deref().unwrap_or("no path")
                ),
            );
        }
    }

    let has_admin = check_admin_status();
    if !has_admin {
        ui_warn("Elevated shell required for boot entry creation");
        ui_kv(
            "Run later as Admin",
            "partboot boot-entry create --esp <path> --root <path> --label PartBoot",
        );
        return Ok(());
    }

    if matching.is_empty() {
        print!("\nCreate a persistent UEFI boot entry for PartBoot? [y/N]: ");
        io::stdout().flush().map_err(|error| error.to_string())?;
    } else {
        if stale.is_empty() {
            print!("\nBoot entry already exists. Options: [R]eplace, [S]kip: ");
        } else {
            print!("\nBoot entry exists (with stale entries). Options: [R]eplace, [C]lean stale, [B]oth, [S]kip: ");
        }
        io::stdout().flush().map_err(|error| error.to_string())?;
    }
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| error.to_string())?;
    let entered = input.trim().to_ascii_lowercase();

    if matching.is_empty() {
        if !matches!(entered.as_str(), "y" | "yes") {
            ui_warn("Boot entry creation skipped");
            ui_kv(
                "Run later",
                "partboot boot-entry create --esp <path> --root <path> --label PartBoot",
            );
            return Ok(());
        }
    } else {
        let do_replace = matches!(entered.as_str(), "r" | "replace");
        let do_clean = matches!(entered.as_str(), "c" | "clean");
        let do_both = matches!(entered.as_str(), "b" | "both");

        if do_clean || do_both {
            for entry in &stale {
                ui_ok(&format!("Removing stale entry {}...", entry.identifier));
                remove_boot_entry(&entry.identifier, false)?;
                ui_ok(&format!("Removed stale entry {}", entry.identifier));
            }
        }
        if !do_replace && !do_both {
            ui_warn("Boot entry update skipped");
            return Ok(());
        }
        ui_ok("Replacing existing boot entry...");
    }

    ui_ok(&format!("Creating firmware boot entry '{label}'..."));

    let result = create_boot_entry(esp, Some(root), label, None, true, false)?;

    if result.reused_existing {
        ui_ok(&format!(
            "Reused existing firmware entry {}",
            result.identifier
        ));
    } else {
        ui_ok(&format!("Created firmware entry {}", result.identifier));
    }
    ui_kv("Label", &result.label);
    ui_kv("Loader", &result.loader);
    ui_kv("Placement", "first (top of boot order)");
    if let Some(path) = &result.backup_path {
        ui_kv("BCD backup", &path.display().to_string());
        ui_kv(
            "Restore command",
            &format!("partboot boot-entry restore --backup {}", path.display()),
        );
    }
    ui_ok("Boot entry created successfully");
    Ok(())
}

#[cfg(windows)]
fn check_admin_status() -> bool {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[bool]([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)",
        ])
        .output();
    match output {
        Ok(output) => {
            let value = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_ascii_lowercase();
            output.status.success() && value == "true"
        }
        Err(_) => false,
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

fn validate_partition_uuid_for_root(root: &Path, partition_uuid: &str) -> Result<(), String> {
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
    let linux_images: Vec<_> = images
        .into_iter()
        .filter(|image| is_supported_linux_family(&image.family))
        .collect();
    if linux_images.is_empty() {
        return Ok("n/a (no supported Linux ISO found)".to_string());
    }

    let complete = linux_images
        .iter()
        .filter(|image| {
            is_complete_extracted(
                layout,
                &crate::extract::extracted_id_from_iso_name(&image.name),
            )
        })
        .count();
    if complete == linux_images.len() {
        Ok("yes".to_string())
    } else {
        Ok(format!("no ({complete}/{})", linux_images.len()))
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
    esp: &Path,
    dry_run: bool,
    force: bool,
) -> Result<(), String> {
    if !dry_run && !force {
        return Err("install-esp requires --dry or --force".to_string());
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
    ui_kv("Source", &staged.display().to_string());
    ui_kv("Destination", &destination.display().to_string());

    if dry_run {
        ui_kv(
            "Dry-run",
            &format!("would create {}", destination.display()),
        );
        ui_kv("Dry-run", "would copy grubx64.efi");
        ui_kv("Dry-run", "would copy grub.cfg");
        ui_kv("Dry-run", "no files changed");
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
    ui_ok(&format!("Installed EFI files in {}", destination.display()));
    ui_kv("Firmware entries", "not modified");
    Ok(())
}

fn install_fallback(
    layout: &PartBootLayout,
    esp: &Path,
    dry_run: bool,
    force: bool,
) -> Result<(), String> {
    if !dry_run && !force {
        return Err("install-fallback requires --dry or --force".to_string());
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
    ui_kv("Source", &staged.display().to_string());
    ui_kv("Fallback destination", &destination.display().to_string());

    if destination_boot.exists() && !dry_run && !force {
        return Err(format!(
            "{} already exists; rerun with --force only if this is a disposable ESP",
            destination_boot.display()
        ));
    }

    if dry_run {
        ui_kv(
            "Dry-run",
            &format!("would create {}", destination.display()),
        );
        ui_kv("Dry-run", "would copy bootx64.efi");
        ui_kv("Dry-run", "would copy grubx64.efi");
        ui_kv("Dry-run", "would copy grub.cfg");
        ui_kv("Dry-run", "no files changed");
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
    ui_ok(&format!(
        "Installed fallback EFI files in {}",
        destination.display()
    ));
    ui_kv(
        "Next",
        "reboot and choose the UEFI entry for this disk/partition",
    );
    Ok(())
}

fn validate_esp_filesystem(esp: &Path) -> Result<(), String> {
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

fn print_boot_instructions(esp: &Path) -> Result<(), String> {
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

fn drive_from_path(path: &Path) -> Result<String, String> {
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
            println!("use: --uuid {uuid}");
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn clap_parse_init_command() {
        let cli = Cli::parse_from(["partboot", "init", "--root", "X:/partboot"]);
        assert_eq!(
            cli.command,
            Some(Command::Init {
                root: PathBuf::from("X:/partboot")
            })
        );
    }

    #[test]
    fn clap_parse_scan_command_with_json() {
        let cli = Cli::parse_from(["partboot", "scan", "--root", "H:/partboot", "--json"]);
        assert_eq!(
            cli.command,
            Some(Command::Scan {
                root: PathBuf::from("H:/partboot"),
                json: true
            })
        );
    }

    #[test]
    fn clap_parse_generate_menu_command() {
        let cli = Cli::parse_from([
            "partboot",
            "menu",
            "--root",
            "X:/partboot",
            "--uuid",
            "ABCD-1234",
            "--output",
            "X:/partboot/generated/grub.cfg",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Menu {
                root: PathBuf::from("X:/partboot"),
                partition_uuid: "ABCD-1234".to_string(),
                partition_label: None,
                include_diagnostics: false,
                json: false,
                output: Some(PathBuf::from("X:/partboot/generated/grub.cfg")),
            })
        );
    }

    #[test]
    fn clap_parse_extract_command() {
        let cli = Cli::parse_from([
            "partboot",
            "extract",
            "--root",
            "H:/partboot",
            "--iso",
            "ubuntu-22.04.5-desktop-amd64.iso",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Extract {
                root: PathBuf::from("H:/partboot"),
                iso: "ubuntu-22.04.5-desktop-amd64.iso".to_string()
            })
        );
    }

    #[test]
    fn clap_parse_volume_id_command() {
        let cli = Cli::parse_from(["partboot", "vol", "--drive", "H:"]);
        assert_eq!(
            cli.command,
            Some(Command::Vol {
                drive: "H:".to_string()
            })
        );
    }

    #[test]
    fn clap_parse_stage_efi_command() {
        let cli = Cli::parse_from([
            "partboot",
            "stage",
            "--root",
            "X:/partboot",
            "--grub-x64",
            "C:/tmp/grubx64.efi",
            "--output",
            "X:/partboot/efi",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Stage {
                root: PathBuf::from("X:/partboot"),
                grub_x64: PathBuf::from("C:/tmp/grubx64.efi"),
                boot_x64: None,
                output: Some(PathBuf::from("X:/partboot/efi"))
            })
        );
    }

    #[test]
    fn clap_parse_install_esp_command() {
        let cli = Cli::parse_from([
            "partboot",
            "esp",
            "--root",
            "H:/partboot",
            "--esp",
            "S:/",
            "--dry-run",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Esp {
                root: PathBuf::from("H:/partboot"),
                esp: PathBuf::from("S:/"),
                dry_run: true,
                force: false
            })
        );
    }

    #[test]
    fn clap_parse_install_fallback_command() {
        let cli = Cli::parse_from([
            "partboot",
            "fallback",
            "--root",
            "H:/partboot",
            "--esp",
            "S:/",
            "--force",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Fallback {
                root: PathBuf::from("H:/partboot"),
                esp: PathBuf::from("S:/"),
                dry_run: false,
                force: true
            })
        );
    }

    #[test]
    fn clap_parse_boot_instructions_command() {
        let cli = Cli::parse_from(["partboot", "boot", "--esp", "S:/"]);
        assert_eq!(
            cli.command,
            Some(Command::Boot {
                esp: PathBuf::from("S:/")
            })
        );
    }

    #[test]
    fn clap_parse_generate_menu_with_diagnostics_flag() {
        let cli = Cli::parse_from([
            "partboot",
            "menu",
            "--root",
            "X:/partboot",
            "--uuid",
            "ABCD-1234",
            "--diagnostics",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Menu {
                root: PathBuf::from("X:/partboot"),
                partition_uuid: "ABCD-1234".to_string(),
                partition_label: None,
                include_diagnostics: true,
                json: false,
                output: None,
            })
        );
    }

    #[test]
    fn clap_parse_doctor_command_with_esp() {
        let cli = Cli::parse_from([
            "partboot",
            "doctor",
            "--root",
            "H:/partboot",
            "--esp",
            "S:/",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Doctor {
                root: PathBuf::from("H:/partboot"),
                esp: Some(PathBuf::from("S:/")),
                json: false
            })
        );
    }

    #[test]
    fn clap_parse_doctor_command_with_json() {
        let cli = Cli::parse_from(["partboot", "doctor", "--root", "H:/partboot", "--json"]);
        assert_eq!(
            cli.command,
            Some(Command::Doctor {
                root: PathBuf::from("H:/partboot"),
                esp: None,
                json: true
            })
        );
    }

    #[test]
    fn clap_parse_start_command_scripted() {
        let cli = Cli::parse_from([
            "partboot",
            "start",
            "--root",
            "H:/partboot",
            "--esp",
            "S:/",
            "--uuid",
            "9412B8E612B8CF0C",
            "--label",
            "partboottest",
            "--diagnostics",
            "--dry-run",
            "--json",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Start {
                root: Some(PathBuf::from("H:/partboot")),
                esp: Some(PathBuf::from("S:/")),
                partition_uuid: Some("9412B8E612B8CF0C".to_string()),
                partition_label: Some("partboottest".to_string()),
                iso: None,
                include_diagnostics: true,
                dry_run_install: true,
                skip_boot_entry: false,
                json: true
            })
        );
    }

    #[test]
    fn clap_parse_start_command_interactive() {
        let cli = Cli::parse_from(["partboot", "start", "--diagnostics", "--dry-run"]);
        assert_eq!(
            cli.command,
            Some(Command::Start {
                root: None,
                esp: None,
                partition_uuid: None,
                partition_label: None,
                iso: None,
                include_diagnostics: true,
                dry_run_install: true,
                skip_boot_entry: false,
                json: false
            })
        );
    }

    #[test]
    fn clap_parse_start_command_with_skip_boot_entry() {
        let cli = Cli::parse_from(["partboot", "start", "--skip-entry"]);
        assert_eq!(
            cli.command,
            Some(Command::Start {
                root: None,
                esp: None,
                partition_uuid: None,
                partition_label: None,
                iso: None,
                include_diagnostics: false,
                dry_run_install: false,
                skip_boot_entry: true,
                json: false
            })
        );
    }

    #[test]
    fn clap_no_args_defaults_to_start() {
        let cli = Cli::parse_from(["partboot"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn clap_parse_boot_entry_list_command() {
        let cli = Cli::parse_from(["partboot", "entry", "list", "--partboot", "--json"]);
        assert_eq!(
            cli.command,
            Some(Command::BootEntry(BootEntryCommand::List {
                partboot_only: true,
                json: true
            }))
        );
    }

    #[test]
    fn clap_parse_boot_entry_create_command() {
        let cli = Cli::parse_from([
            "partboot",
            "entry",
            "create",
            "--esp",
            "S:/",
            "--label",
            "PartBoot",
            "--loader",
            "\\EFI\\PartBoot\\grubx64.efi",
            "--first",
            "--dry-run",
            "--json",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::BootEntry(BootEntryCommand::Create {
                esp: PathBuf::from("S:/"),
                root: None,
                label: "PartBoot".to_string(),
                loader: Some("\\EFI\\PartBoot\\grubx64.efi".to_string()),
                first: true,
                dry_run: true,
                json: true
            }))
        );
    }

    #[test]
    fn clap_parse_boot_entry_create_command_with_root() {
        let cli = Cli::parse_from([
            "partboot",
            "entry",
            "create",
            "--esp",
            "S:/",
            "--root",
            "H:/partboot",
            "--label",
            "PartBoot",
            "--dry-run",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::BootEntry(BootEntryCommand::Create {
                esp: PathBuf::from("S:/"),
                root: Some(PathBuf::from("H:/partboot")),
                label: "PartBoot".to_string(),
                loader: None,
                first: false,
                dry_run: true,
                json: false
            }))
        );
    }

    #[test]
    fn clap_parse_boot_entry_remove_command() {
        let cli = Cli::parse_from([
            "partboot",
            "entry",
            "remove",
            "--id",
            "{12345678-1234-1234-1234-123456789ABC}",
            "--dry-run",
            "--json",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::BootEntry(BootEntryCommand::Remove {
                id: "{12345678-1234-1234-1234-123456789ABC}".to_string(),
                dry_run: true,
                json: true
            }))
        );
    }

    #[test]
    fn clap_parse_boot_entry_restore_command() {
        let cli = Cli::parse_from([
            "partboot",
            "entry",
            "restore",
            "--backup",
            "C:/temp/partboot-bcd-backup.bak",
            "--dry-run",
            "--json",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::BootEntry(BootEntryCommand::Restore {
                backup: PathBuf::from("C:/temp/partboot-bcd-backup.bak"),
                dry_run: true,
                json: true
            }))
        );
    }
}
