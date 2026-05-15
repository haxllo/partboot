use crate::iso::{IsoFamily, IsoImage};
use crate::layout::display_path;
use crate::profile::IsoProfile;

pub fn generate_grub_cfg(
    images: &[IsoImage],
    partition_uuid: &str,
    partition_label: Option<&str>,
    include_diagnostics: bool,
    _profiles: &[IsoProfile],
) -> String {
    let mut cfg = String::new();
    cfg.push_str(&header());
    cfg.push_str(&format!(
        "search --no-floppy --fs-uuid --set=partboot_root {}\n\n",
        escape_grub(partition_uuid)
    ));
    if let Some(label) = partition_label {
        cfg.push_str("if [ -z \"$partboot_root\" ]; then\n");
        cfg.push_str(&format!(
            "    search --no-floppy --label --set=partboot_root '{}'\n",
            escape_grub(label)
        ));
        cfg.push_str("fi\n\n");
    }
    cfg.push_str(&format!(
        "set partboot_uuid='{}'\n\n",
        escape_grub(partition_uuid)
    ));

    if images.is_empty() {
        cfg.push_str("menuentry 'No ISO images found' {\n");
        cfg.push_str("    echo 'Copy ISO files into /partboot/isos and regenerate grub.cfg.'\n");
        cfg.push_str("    sleep 5\n");
        cfg.push_str("}\n");
        return cfg;
    }

    for image in images {
        cfg.push_str(&entry_for_image(image));
        cfg.push('\n');
    }
    if include_diagnostics {
        cfg.push_str(&diagnostics_entry(partition_uuid, partition_label));
    }

    cfg
}

fn header() -> String {
    [
        "set timeout=10",
        "set default=0",
        "set menu_color_normal=white/black",
        "set menu_color_highlight=black/light-gray",
        "",
        "insmod part_gpt",
        "insmod fat",
        "insmod ntfs",
        "insmod ext2",
        "insmod iso9660",
        "insmod loopback",
        "insmod linux",
        "insmod search_fs_uuid",
        "",
    ]
    .join("\n")
}

fn entry_for_image(image: &IsoImage) -> String {
    match image.family {
        IsoFamily::UbuntuCasper => ubuntu_entry(image),
        IsoFamily::DebianLive => debian_entry(image),
        IsoFamily::Arch => arch_entry(image),
        IsoFamily::Fedora => fedora_entry(image),
        IsoFamily::Windows => unsupported_entry(
            image,
            "Windows ISO boot needs a wimboot or extracted-installer backend, which is not enabled in this MVP.",
        ),
        IsoFamily::Unknown => unsupported_entry(
            image,
            "No boot profile matched this ISO. Add a profile before trying to boot it.",
        ),
    }
}

fn iso_grub_path(image: &IsoImage) -> String {
    format!("/partboot/isos/{}", escape_grub(&image.name))
}

fn loop_device_name(image: &IsoImage) -> String {
    let mut name = String::from("loop_");
    for ch in image.name.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' || ch == '.' {
            name.push('_');
        }
        if name.len() >= 40 {
            break;
        }
    }
    name
}

fn ubuntu_entry(image: &IsoImage) -> String {
    ubuntu_iso_entry(image)
}

fn ubuntu_iso_entry(image: &IsoImage) -> String {
    let iso = iso_grub_path(image);
    let loopdev = loop_device_name(image);
    format!(
        "menuentry '{}' --class ubuntu {{\n    set isofile='{}'\n    loopback {} ($partboot_root)$isofile\n    linux ({})/casper/vmlinuz boot=casper iso-scan/filename=$isofile toram noprompt quiet splash ---\n    initrd ({})/casper/initrd\n}}\n",
        escape_grub(&image.name),
        iso,
        loopdev,
        loopdev,
        loopdev
    )
}

fn debian_entry(image: &IsoImage) -> String {
    let iso = iso_grub_path(image);
    let loopdev = loop_device_name(image);
    format!(
        "menuentry '{}' {{\n    set isofile='{}'\n    loopback {} ($partboot_root)$isofile\n    linux ({})/live/vmlinuz boot=live findiso=$isofile components quiet splash\n    initrd ({})/live/initrd.img\n}}\n",
        escape_grub(&image.name),
        iso,
        loopdev,
        loopdev,
        loopdev
    )
}

fn arch_entry(image: &IsoImage) -> String {
    let iso = iso_grub_path(image);
    let loopdev = loop_device_name(image);
    let paths = arch_boot_paths(&image.name);
    format!(
        "menuentry '{}' {{\n    set isofile='{}'\n    loopback {} ($partboot_root)$isofile\n    linux ({}){} img_dev=/dev/disk/by-uuid/$partboot_uuid img_loop=$isofile archisobasedir=arch\n    initrd ({}){}\n}}\n",
        escape_grub(&image.name),
        iso,
        loopdev,
        loopdev,
        paths.kernel,
        loopdev,
        paths.initrd
    )
}

struct ArchBootPaths {
    kernel: &'static str,
    initrd: &'static str,
}

fn arch_boot_paths(iso_name: &str) -> ArchBootPaths {
    let lower = iso_name.to_ascii_lowercase();
    if lower.contains("omarchy") {
        return ArchBootPaths {
            kernel: "/arch/boot/x86_64/vmlinuz-linux-t2",
            initrd: "/arch/boot/x86_64/initramfs-linux-t2.img",
        };
    }
    if lower.contains("cachyos") {
        return ArchBootPaths {
            kernel: "/arch/boot/x86_64/vmlinuz-linux-cachyos",
            initrd: "/arch/boot/x86_64/initramfs-linux-cachyos.img",
        };
    }
    ArchBootPaths {
        kernel: "/arch/boot/x86_64/vmlinuz-linux",
        initrd: "/arch/boot/x86_64/initramfs-linux.img",
    }
}

fn fedora_entry(image: &IsoImage) -> String {
    let iso = iso_grub_path(image);
    let loopdev = loop_device_name(image);
    format!(
        "menuentry '{}' {{\n    set isofile='{}'\n    loopback {} ($partboot_root)$isofile\n    if [ -f ({})/images/pxeboot/vmlinuz ]; then\n        linux ({})/images/pxeboot/vmlinuz iso-scan/filename=$isofile root=live:CDLABEL=Fedora rd.live.image quiet rhgb\n        initrd ({})/images/pxeboot/initrd.img\n    elif [ -f ({})/isolinux/vmlinuz ]; then\n        linux ({})/isolinux/vmlinuz iso-scan/filename=$isofile root=live:CDLABEL=Fedora rd.live.image quiet rhgb\n        initrd ({})/isolinux/initrd.img\n    elif [ -f ({})/boot/x86_64/loader/linux ]; then\n        linux ({})/boot/x86_64/loader/linux iso-scan/filename=$isofile root=live:CDLABEL=Fedora rd.live.image quiet rhgb\n        initrd ({})/boot/x86_64/loader/initrd\n    else\n        echo 'Unsupported Fedora ISO layout for direct ISO boot.'\n        sleep 8\n    fi\n}}\n",
        escape_grub(&image.name),
        iso,
        loopdev,
        loopdev,
        loopdev,
        loopdev,
        loopdev,
        loopdev,
        loopdev,
        loopdev,
        loopdev,
        loopdev
    )
}

fn unsupported_entry(image: &IsoImage, message: &str) -> String {
    format!(
        "menuentry '{}' {{\n    echo '{}'\n    echo '{}'\n    echo '{}'\n    sleep 8\n}}\n",
        escape_grub(&image.name),
        escape_grub(&format!("path: {}", display_path(&image.path))),
        escape_grub(message),
        escape_grub("Boot skipped.")
    )
}

fn diagnostics_entry(partition_uuid: &str, partition_label: Option<&str>) -> String {
    format!(
        "menuentry 'PartBoot diagnostics' {{\n    echo 'PartBoot diagnostics'\n    echo 'partboot_root='$partboot_root\n    echo 'partboot_uuid={}'\n    echo 'partition_label={}'\n    echo 'expected ISO directory: /partboot/isos'\n    echo 'expected extracted directory: /partboot/extracted'\n    echo 'Press Escape to return to the menu.'\n    sleep --interruptible 30\n}}\n",
        escape_grub(partition_uuid),
        escape_grub(partition_label.unwrap_or("(none)"))
    )
}

fn escape_grub(value: &str) -> String {
    value.replace('\\', "/").replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso::{IsoFamily, IsoImage, SupportLevel};
    use std::path::PathBuf;

    #[test]
    fn ubuntu_grub_entry_contains_loopback_boot() {
        let image = IsoImage {
            name: "ubuntu-24.04.iso".to_string(),
            path: PathBuf::from("X:/partboot/isos/ubuntu-24.04.iso"),
            family: IsoFamily::UbuntuCasper,
            support: SupportLevel::Supported,
            extracted_id: None,
        };
        let cfg = generate_grub_cfg(&[image], "ABCD-1234", Some("partboottest"), false, &[]);
        assert!(cfg.contains("set menu_color_normal=white/black"));
        assert!(!cfg.contains("menuentry 'PartBoot ISO Manager'"));
        assert!(cfg.contains("loopback loop_ubuntu_24_04_iso"));
        assert!(cfg.contains("linux (loop_ubuntu_24_04_iso)/casper/vmlinuz"));
        assert!(cfg.contains("iso-scan/filename=$isofile"));
        assert!(cfg.contains("toram noprompt quiet splash"));
        assert!(cfg.contains("search --no-floppy --fs-uuid --set=partboot_root ABCD-1234"));
        assert!(cfg.contains("search --no-floppy --label --set=partboot_root 'partboottest'"));
        assert!(cfg.contains("menuentry 'ubuntu-24.04.iso' --class ubuntu"));
        assert!(!cfg.contains("Ubuntu live mode copied into RAM"));
        assert!(!cfg.contains("PartBoot diagnostics"));
    }

    #[test]
    fn ubuntu_grub_entry_with_extracted_still_uses_single_iso_entry() {
        let image = IsoImage {
            name: "ubuntu-24.04.iso".to_string(),
            path: PathBuf::from("X:/partboot/isos/ubuntu-24.04.iso"),
            family: IsoFamily::UbuntuCasper,
            support: SupportLevel::Supported,
            extracted_id: Some("ubuntu-24.04".to_string()),
        };
        let cfg = generate_grub_cfg(&[image], "ABCD-1234", Some("partboottest"), false, &[]);
        assert!(cfg.contains("menuentry 'ubuntu-24.04.iso' --class ubuntu"));
        assert!(cfg.contains("toram noprompt quiet splash"));
        assert!(!cfg.contains("submenu 'ubuntu-24.04.iso'"));
        assert!(!cfg.contains("live-media-path=$extracted"));
    }

    #[test]
    fn all_supported_families_use_single_entries() {
        let images = vec![
            IsoImage {
                name: "ubuntu-24.04.iso".to_string(),
                path: PathBuf::from("X:/partboot/isos/ubuntu-24.04.iso"),
                family: IsoFamily::UbuntuCasper,
                support: SupportLevel::Supported,
                extracted_id: None,
            },
            IsoImage {
                name: "debian-12-live.iso".to_string(),
                path: PathBuf::from("X:/partboot/isos/debian-12-live.iso"),
                family: IsoFamily::DebianLive,
                support: SupportLevel::Supported,
                extracted_id: None,
            },
            IsoImage {
                name: "archlinux-2026.iso".to_string(),
                path: PathBuf::from("X:/partboot/isos/archlinux-2026.iso"),
                family: IsoFamily::Arch,
                support: SupportLevel::Supported,
                extracted_id: None,
            },
            IsoImage {
                name: "fedora-workstation.iso".to_string(),
                path: PathBuf::from("X:/partboot/isos/fedora-workstation.iso"),
                family: IsoFamily::Fedora,
                support: SupportLevel::Supported,
                extracted_id: None,
            },
        ];
        let cfg = generate_grub_cfg(&images, "ABCD-1234", None, false, &[]);
        assert!(cfg.contains("menuentry 'ubuntu-24.04.iso' --class ubuntu"));
        assert!(cfg.contains("menuentry 'debian-12-live.iso'"));
        assert!(cfg.contains("menuentry 'archlinux-2026.iso'"));
        assert!(cfg.contains("menuentry 'fedora-workstation.iso'"));
        assert!(!cfg.contains("submenu '"));
    }

    #[test]
    fn non_ubuntu_families_use_iso_normal_mode_even_when_extracted_exists() {
        let images = vec![
            IsoImage {
                name: "debian-live.iso".to_string(),
                path: PathBuf::from("X:/partboot/isos/debian-live.iso"),
                family: IsoFamily::DebianLive,
                support: SupportLevel::Supported,
                extracted_id: Some("debian-live".to_string()),
            },
            IsoImage {
                name: "archlinux.iso".to_string(),
                path: PathBuf::from("X:/partboot/isos/archlinux.iso"),
                family: IsoFamily::Arch,
                support: SupportLevel::Supported,
                extracted_id: Some("archlinux".to_string()),
            },
            IsoImage {
                name: "fedora-live.iso".to_string(),
                path: PathBuf::from("X:/partboot/isos/fedora-live.iso"),
                family: IsoFamily::Fedora,
                support: SupportLevel::Supported,
                extracted_id: Some("fedora-live".to_string()),
            },
        ];
        let cfg = generate_grub_cfg(&images, "ABCD-1234", None, false, &[]);
        assert!(cfg.contains("menuentry 'debian-live.iso'"));
        assert!(cfg.contains("linux (loop_debian_live_iso)/live/vmlinuz"));
        assert!(cfg.contains("menuentry 'archlinux.iso'"));
        assert!(cfg.contains("linux (loop_archlinux_iso)/arch/boot/x86_64/vmlinuz-linux"));
        assert!(cfg.contains("menuentry 'fedora-live.iso'"));
        assert!(cfg.contains("loopback loop_fedora_live_iso"));
    }

    #[test]
    fn fedora_grub_entry_includes_kernel_path_fallbacks() {
        let image = IsoImage {
            name: "fedora-workstation.iso".to_string(),
            path: PathBuf::from("X:/partboot/isos/fedora-workstation.iso"),
            family: IsoFamily::Fedora,
            support: SupportLevel::Supported,
            extracted_id: None,
        };
        let cfg = generate_grub_cfg(&[image], "ABCD-1234", None, false, &[]);
        assert!(cfg.contains("loopback loop_fedora_workstation_iso"));
        assert!(cfg.contains("if [ -f (loop_fedora_workstation_iso)/images/pxeboot/vmlinuz ]; then"));
        assert!(cfg.contains("elif [ -f (loop_fedora_workstation_iso)/isolinux/vmlinuz ]; then"));
        assert!(cfg.contains("elif [ -f (loop_fedora_workstation_iso)/boot/x86_64/loader/linux ]; then"));
    }

    #[test]
    fn diagnostics_entry_is_optional() {
        let image = IsoImage {
            name: "ubuntu-24.04.iso".to_string(),
            path: PathBuf::from("X:/partboot/isos/ubuntu-24.04.iso"),
            family: IsoFamily::UbuntuCasper,
            support: SupportLevel::Supported,
            extracted_id: None,
        };
        let cfg = generate_grub_cfg(&[image], "ABCD-1234", Some("partboottest"), true, &[]);
        assert!(cfg.contains("PartBoot diagnostics"));
        assert!(cfg.contains("partboot_uuid"));
        assert!(cfg.contains("partboot_root"));
    }

    #[test]
    fn arch_grub_entry_uses_standard_arch_kernel_by_default() {
        let image = IsoImage {
            name: "archlinux-2026.01.01-x86_64.iso".to_string(),
            path: PathBuf::from("X:/partboot/isos/archlinux-2026.01.01-x86_64.iso"),
            family: IsoFamily::Arch,
            support: SupportLevel::Supported,
            extracted_id: None,
        };
        let cfg = generate_grub_cfg(&[image], "D826AD8826AD67E8", None, false, &[]);
        assert!(cfg.contains("(loop_archlinux_2026_01_01_x86_64_iso)/arch/boot/x86_64/vmlinuz-linux "));
        assert!(cfg.contains("(loop_archlinux_2026_01_01_x86_64_iso)/arch/boot/x86_64/initramfs-linux.img"));
    }

    #[test]
    fn arch_grub_entry_supports_omarchy_kernel_names() {
        let image = IsoImage {
            name: "omarchy-3.2.3-2.iso".to_string(),
            path: PathBuf::from("X:/partboot/isos/omarchy-3.2.3-2.iso"),
            family: IsoFamily::Arch,
            support: SupportLevel::Supported,
            extracted_id: None,
        };
        let cfg = generate_grub_cfg(&[image], "D826AD8826AD67E8", None, false, &[]);
        assert!(cfg.contains("(loop_omarchy_3_2_3_2_iso)/arch/boot/x86_64/vmlinuz-linux-t2 "));
        assert!(cfg.contains("(loop_omarchy_3_2_3_2_iso)/arch/boot/x86_64/initramfs-linux-t2.img"));
        assert!(cfg.contains("archisobasedir=arch"));
    }

    #[test]
    fn arch_grub_entry_supports_cachyos_kernel_names() {
        let image = IsoImage {
            name: "cachyos-desktop-linux-250713.iso".to_string(),
            path: PathBuf::from("X:/partboot/isos/cachyos-desktop-linux-250713.iso"),
            family: IsoFamily::Arch,
            support: SupportLevel::Supported,
            extracted_id: None,
        };
        let cfg = generate_grub_cfg(&[image], "D826AD8826AD67E8", None, false, &[]);
        assert!(cfg.contains("(loop_cachyos_desktop_linux_250713_iso)/arch/boot/x86_64/vmlinuz-linux-cachyos "));
        assert!(cfg.contains("(loop_cachyos_desktop_linux_250713_iso)/arch/boot/x86_64/initramfs-linux-cachyos.img"));
    }
}
