fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=XNIP_BUILD_COMMIT");
    println!(
        "cargo:rustc-env=XNIP_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
