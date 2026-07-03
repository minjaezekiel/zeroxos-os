# zeroxos build & run targets.
#
# The host simulator (`make sim`) is the fast iteration loop and needs no VM.
# The bare-metal targets (`make build-x86_64`, `make qemu-x86_64`) come online
# with milestone M2 (custom target spec + linker script) and M7 (QEMU boot).

CARGO ?= cargo
ARCH_TARGET := x86_64-unknown-zeroxos
TARGET_JSON := targets/$(ARCH_TARGET).json
KERNEL_BIN  := target/$(ARCH_TARGET)/release/zeroxos-boot

.PHONY: all sim game ipc fs test fmt clippy build-x86_64 qemu-x86_64 clean help

all: test ## Default: run the test suite

help: ## List available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

## --- Host simulator (works today) -------------------------------------------

sim: ## Boot the kernel in the host simulator
	$(CARGO) run -p zerox-sim -- boot

game: ## Gaming-mode scheduler demo
	$(CARGO) run -p zerox-sim -- game

ipc: ## IPC latency benchmark
	$(CARGO) run -p zerox-sim -- ipc

fs: ## Filesystem demo
	$(CARGO) run -p zerox-sim -- fs

## --- Quality ----------------------------------------------------------------

test: ## Run the host test suite (default members — excludes the bare-metal crate)
	$(CARGO) test

fmt: ## Format all crates
	$(CARGO) fmt --all

clippy: ## Lint host crates (default members)
	$(CARGO) clippy --all-targets

## --- Bare metal (x86_64) — from milestone M2 onward -------------------------

build-x86_64: ## Build the bare-metal kernel image (custom target + build-std)
	@if [ ! -f "$(TARGET_JSON)" ]; then \
		echo "[make] $(TARGET_JSON) not present yet — lands in milestone M2 (see ROADMAP.md §2)."; \
		exit 1; \
	fi
	$(CARGO) build -p zeroxos-boot --release \
		--target $(TARGET_JSON) \
		-Z json-target-spec \
		-Z build-std=core,compiler_builtins,alloc \
		-Z build-std-features=compiler-builtins-mem

iso: build-x86_64 ## Build a bootable zeroxos.iso (Limine, BIOS + UEFI)
	./scripts/mk-iso.sh

qemu-iso: iso ## Boot the ISO in QEMU (BIOS) with a serial console
	qemu-system-x86_64 \
		-cdrom zeroxos.iso \
		-m 512M \
		-serial stdio \
		-no-reboot -no-shutdown

qemu-iso-uefi: iso ## Boot the ISO in QEMU under UEFI firmware (needs OVMF at $$OVMF)
	qemu-system-x86_64 \
		-cdrom zeroxos.iso \
		-m 512M \
		-bios $${OVMF:-/usr/local/share/qemu/edk2-x86_64-code.fd} \
		-serial stdio \
		-no-reboot -no-shutdown

clean: ## Remove build artifacts (keeps the fetched Limine bootloader)
	$(CARGO) clean
	rm -rf build/iso_root zeroxos.iso
