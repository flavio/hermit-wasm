SOURCE_FILES := $(shell test -e src/ && find src -type f)

.PHONY: build
build: target/x86_64-unknown-hermit/release/hermit_wasm

target/x86_64-unknown-hermit/release/hermit_wasm: $(SOURCE_FILES) Cargo.* wasm/*.wasm
	cargo build \
		-Zbuild-std=std,panic_abort \
		--target x86_64-unknown-hermit \
		--release

.PHONY: clean
clean:
	cargo clean

.PHONY: run
run: target/x86_64-unknown-hermit/release/hermit_wasm
	qemu-system-x86_64 \
		-cpu qemu64,apic,fsgsbase,fxsr,rdrand,rdtscp,xsave,xsaveopt \
		-display none -serial stdio \
		-smp 8,sockets=1,cores=8,threads=8,maxcpus=64 \
		-m 1G \
		-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
		-kernel rusty-loader-x86_64 \
		-append "-- -r 10.0.2.2 -v" \
		-initrd target/x86_64-unknown-hermit/release/hermit_wasm \
		-netdev user,id=u1,hostfwd=tcp::3000-:3000 \
		-device rtl8139,netdev=u1
