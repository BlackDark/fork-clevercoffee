fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={dir}/ld");
    println!("cargo:rerun-if-changed=ld");
    println!("cargo:rustc-link-arg=-Trom-fixups.x");
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}
