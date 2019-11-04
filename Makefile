target := aarch64-proton
image := kernel8.img
raspi ?= 3
profile = $(if $(release),release,debug)

build:
	@cargo xbuild $(if $(release), --release) --target $(target).json --features raspi$(raspi)
	@cargo objcopy -- ./target/$(target)/$(profile)/proton -O binary ./kernel8.img

run: build kernel8.img objdump
	@qemu-system-aarch64 -M raspi3 -serial stdio -kernel ./kernel8.img -d int

objdump:
	@llvm-objdump -d ./target/$(target)/$(profile)/proton > kernel.S

debug: build kernel8.img objdump
	@qemu-system-aarch64 -M raspi3 -serial stdio -kernel ./kernel8.img  -d int -s -S

gdb:
	@gdb-multiarch "target/aarch64-proton/$(profile)/proton" -ex "set arch aarch64" -ex "target remote :1234"
