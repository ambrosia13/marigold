TARGET_DIR = debug
CARGO_FLAGS = # empty by default to represent debug mode
SHADER_DEBUG_INFO = 0 # no debug info by default

debug: TARGET_DIR = debug
debug: CARGO_FLAGS = 
debug: SHADER_DEBUG_INFO = 0

release: TARGET_DIR = release
release: CARGO_FLAGS = --release
release: SHADER_DEBUG_INFO = 0

# for some reason, nsight only works with release builds
nsight: TARGET_DIR = release
nsight: CARGO_FLAGS = --release
nsight: SHADER_DEBUG_INFO = 1

.PHONY: debug release nsight build bundle clean

debug release nsight: build

build: 
	SHADER_DEBUG_INFO=$(SHADER_DEBUG_INFO) cargo build $(CARGO_FLAGS)

	mkdir -p out/marigold
	cp target/$(TARGET_DIR)/marigold out/marigold/marigold

	mkdir -p out/marigold/assets
	mkdir -p out/marigold/assets/shaders

	cp -r assets/meshes out/marigold/assets
	cp -r assets/shaders/target out/marigold/assets/shaders

bundle: release
	tar czf out/marigold.tar.gz -C out marigold

clean:
	rm -rf out/

