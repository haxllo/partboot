#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${REPO_ROOT}/target/efi-smoke"
ESP_DIR="${WORK_DIR}/esp"
LOG_PATH="${WORK_DIR}/qemu.log"
MARKER="PARTBOOT_SMOKE_OK"

find_ovmf_code() {
  local candidates=(
    "/usr/share/OVMF/OVMF_CODE.fd"
    "/usr/share/edk2/ovmf/OVMF_CODE.fd"
    "/usr/share/OVMF/OVMF_CODE_4M.fd"
    "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd"
  )
  for p in "${candidates[@]}"; do
    if [[ -f "$p" ]]; then
      echo "$p"
      return 0
    fi
  done
  return 1
}

OVMF_CODE="$(find_ovmf_code || true)"
if [[ -z "${OVMF_CODE}" ]]; then
  echo "OVMF firmware file not found on runner."
  exit 1
fi

rm -rf "$WORK_DIR"
mkdir -p "${ESP_DIR}/EFI/BOOT"

cp "${REPO_ROOT}/assets/efi/grubx64.efi" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI"
cat > "${ESP_DIR}/EFI/BOOT/grub.cfg" <<'CFG'
set timeout=1
set default=0
menuentry "PartBoot smoke" {
  echo PARTBOOT_SMOKE_OK
  sleep 1
  halt
}
CFG

set +e
timeout 60 qemu-system-x86_64 \
  -nodefaults \
  -machine q35,accel=tcg \
  -m 512 \
  -nographic \
  -serial stdio \
  -drive if=pflash,format=raw,readonly=on,file="${OVMF_CODE}" \
  -drive format=raw,file=fat:rw:"${ESP_DIR}" \
  > "${LOG_PATH}" 2>&1
QEMU_EXIT=$?
set -e

if ! grep -q "${MARKER}" "${LOG_PATH}"; then
  echo "EFI smoke test failed: marker not found (${MARKER})."
  echo "Last log lines:"
  tail -n 200 "${LOG_PATH}" || true
  exit 1
fi

echo "EFI smoke test passed."
exit 0
