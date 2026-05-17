TARGET_DIR = debug

# empty by default to represent debug mode
CARGO_FLAGS =

# no debug info by default
SHADER_DEBUG_INFO = 0

debug: TARGET_DIR = debug
debug: CARGO_FLAGS =
debug: SHADER_DEBUG_INFO = 0

release: TARGET_DIR = release
release: CARGO_FLAGS = --release
release: SHADER_DEBUG_INFO = 0

# enable shader debug info in slangc for nsight builds
nsight: TARGET_DIR = debug
nsight: CARGO_FLAGS =
nsight: SHADER_DEBUG_INFO = 1

.PHONY: debug release nsight build bundle clean

debug release nsight: build

build:
	SHADER_DEBUG_INFO=$(SHADER_DEBUG_INFO) cargo build $(CARGO_FLAGS) -p marigold

	mkdir -p out/marigold
	cp target/$(TARGET_DIR)/marigold out/marigold/marigold

	mkdir -p out/marigold/assets
	mkdir -p out/marigold/assets/shaders

	cp -r marigold/assets/meshes out/marigold/assets
	cp -r marigold/assets/shaders/target out/marigold/assets/shaders

bundle: release
	tar czf out/marigold.tar.gz -C out marigold

clean:
	rm -rf out/
