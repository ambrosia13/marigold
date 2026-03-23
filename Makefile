debug:
	cargo build

	mkdir -p out/marigold
	cp target/debug/marigold out/marigold/marigold

	mkdir -p out/marigold/assets
	mkdir -p out/marigold/assets/shaders

	cp -r assets/meshes out/marigold/assets
	cp -r assets/shaders/target out/marigold/assets/shaders

release: 
	cargo build --release

	mkdir -p out/marigold
	cp target/release/marigold out/marigold/marigold

	mkdir -p out/marigold/assets
	mkdir -p out/marigold/assets/shaders

	cp -r assets/meshes out/marigold/assets
	cp -r assets/shaders/target out/marigold/assets/shaders

bundle: release
	tar czf out/marigold.tar.gz -C out marigold

clean:
	rm -rf out/

