use crate::iso::{IsoFamily, IsoImage};
use crate::layout::display_path;
use crate::profile::{default_boot_mode, fallback_boot_mode, BootMode, IsoProfile};

pub fn generate_grub_cfg(
    images: &[IsoImage],
    partition_uuid: &str,
    partition_label: Option<&str>,
    include_diagnostics: bool,
    profiles: &[IsoProfile],
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
        cfg.push_str(&entry_for_image(image, profile_for_image(profiles, image)));
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

fn entry_for_image(image: &IsoImage, profile: Option<&IsoProfile>) -> String {
    match image.family {
        IsoFamily::UbuntuCasper => ubuntu_entry(image, profile),
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

fn profile_for_image<'a>(profiles: &'a [IsoProfile], image: &IsoImage) -> Option<&'a IsoProfile> {
    profiles
        .iter()
        .find(|profile| profile.name.eq_ignore_ascii_case(&image.name))
}

fn ubuntu_entry(image: &IsoImage, profile: Option<&IsoProfile>) -> String {
    let preferred_mode = profile
        .map(|value| value.preferred_mode)
        .unwrap_or_else(|| default_boot_mode(image));
    let fallback_mode = profile
        .map(|value| value.fallback_mode)
        .unwrap_or_else(fallback_boot_mode);
    let visible_fallback = profile.map(|value| value.visible_fallback).unwrap_or(true);

    let primary_mode = if mode_available(preferred_mode, image) {
        preferred_mode
    } else {
        fallback_mode
    };

    let mut entry = render_ubuntu_entry(image, primary_mode, &escape_grub(&image.name));
    if visible_fallback && fallback_mode != primary_mode && mode_available(fallback_mode, image) {
        entry.push('\n');
        entry.push_str(&render_ubuntu_entry(
            image,
            fallback_mode,
            &format!("{} [Fallback]", escape_grub(&image.name)),
        ));
    }
    entry
}

fn mode_available(mode: BootMode, image: &IsoImage) -> bool {
    match mode {
        BootMode::Extracted => image.extracted_id.is_some(),
        BootMode::IsoToram => true,
    }
}

fn render_ubuntu_entry(image: &IsoImage, mode: BootMode, label: &str) -> String {
    match mode {
        BootMode::Extracted => ubuntu_extracted_entry(image, label),
        BootMode::IsoToram => ubuntu_iso_toram_entry(image, label),
    }
}

fn ubuntu_iso_toram_entry(image: &IsoImage, label: &str) -> String {
    let iso = iso_grub_path(image);
    format!(
        "menuentry '{}' --class ubuntu {{\n    set isofile='{}'\n    echo 'Ubuntu live mode copied into RAM for clean shutdown'\n    loopback loop ($partboot_root)$isofile\n    linux (loop)/casper/vmlinuz boot=casper iso-scan/filename=$isofile toram noprompt quiet splash ---\n    initrd (loop)/casper/initrd\n}}\n",
        escape_grub(label),
        iso
    )
}

fn ubuntu_extracted_entry(image: &IsoImage, label: &str) -> String {
    let extracted_id = image.extracted_id.as_deref().unwrap_or("");
    let casper_path = format!("/partboot/extracted/{}/casper", escape_grub(extracted_id));
    format!(
        "menuentry '{}' --class ubuntu {{\n    echo 'Ubuntu extracted Casper mode'\n    linux ($partboot_root){}/vmlinuz boot=casper live-media=/dev/disk/by-uuid/$partboot_uuid live-media-path={} ignore_uuid noprompt quiet splash ---\n    initrd ($partboot_root){}/initrd\n}}\n",
        escape_grub(label),
        casper_path,
        casper_path,
        casper_path
    )
}

fn debian_entry(image: &IsoImage) -> String {
    let iso = iso_grub_path(image);
    format!(
        "menuentry '{}' {{\n    set isofile='{}'\n    loopback loop ($partboot_root)$isofile\n    linux (loop)/live/vmlinuz boot=live findiso=$isofile components quiet splash\n    initrd (loop)/live/initrd.img\n}}\n",
        escape_grub(&image.name),
        iso
    )
}

fn arch_entry(image: &IsoImage) -> String {
    let iso = iso_grub_path(image);
    format!(
        "menuentry '{}' {{\n    set isofile='{}'\n    loopback loop ($partboot_root)$isofile\n    linux (loop)/arch/boot/x86_64/vmlinuz-linux img_dev=/dev/disk/by-uuid/$partboot_uuid img_loop=$isofile archisobasedir=arch\n    initrd (loop)/arch/boot/x86_64/initramfs-linux.img\n}}\n",
        escape_grub(&image.name),
        iso
    )
}

fn fedora_entry(image: &IsoImage) -> String {
    let iso = iso_grub_path(image);
    format!(
        "menuentry '{}' {{\n    set isofile='{}'\n    loopback loop ($partboot_root)$isofile\n    linux (loop)/images/pxeboot/vmlinuz iso-scan/filename=$isofile root=live:CDLABEL=Fedora quiet\n    initrd (loop)/images/pxeboot/initrd.img\n}}\n",
        escape_grub(&image.name),
        iso
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
        assert!(cfg.contains("loopback loop"));
        assert!(cfg.contains("linux (loop)/casper/vmlinuz"));
        assert!(cfg.contains("iso-scan/filename=$isofile"));
        assert!(cfg.contains("toram noprompt quiet splash"));
        assert!(cfg.contains("search --no-floppy --fs-uuid --set=partboot_root ABCD-1234"));
        assert!(cfg.contains("search --no-floppy --label --set=partboot_root 'partboottest'"));
        assert!(cfg.contains("menuentry 'ubuntu-24.04.iso' --class ubuntu"));
        assert!(!cfg.contains("PartBoot diagnostics"));
        assert!(!cfg.contains("(safe shutdown)"));
        assert!(!cfg.contains("(debug)"));
    }

    #[test]
    fn ubuntu_grub_entry_prefers_extracted_casper() {
        let image = IsoImage {
            name: "ubuntu-24.04.iso".to_string(),
            path: PathBuf::from("X:/partboot/isos/ubuntu-24.04.iso"),
            family: IsoFamily::UbuntuCasper,
            support: SupportLevel::Supported,
            extracted_id: Some("ubuntu-24.04".to_string()),
        };
        let cfg = generate_grub_cfg(&[image], "ABCD-1234", Some("partboottest"), false, &[]);
        assert!(cfg.contains("live-media-path=/partboot/extracted/ubuntu-24.04/casper"));
        assert!(cfg.contains("live-media=/dev/disk/by-uuid/$partboot_uuid"));
        assert!(cfg.contains("ignore_uuid"));
        assert!(cfg.contains("menuentry 'ubuntu-24.04.iso' --class ubuntu"));
        assert!(
            cfg.contains("linux ($partboot_root)/partboot/extracted/ubuntu-24.04/casper/vmlinuz")
        );
        assert!(
            cfg.contains("initrd ($partboot_root)/partboot/extracted/ubuntu-24.04/casper/initrd")
        );
        assert!(cfg.contains("menuentry 'ubuntu-24.04.iso [Fallback]' --class ubuntu"));
        assert!(cfg.contains("loopback loop"));
        assert!(cfg.contains("toram noprompt quiet splash"));
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
}
