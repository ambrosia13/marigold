fn main() {
    println!("cargo:rerun-if-changed=src");

    let build_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    println!("cargo:rustc-env=BVH_BUILD_ID={}", build_id);
}
