// Links the program icon and version information into the Windows .exe.
// packaging/windows/spacemapper.res is built from spacemapper.rc in the same
// folder; rebuild it after changing the icon or the version.
fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/spacemapper.res");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/windows/spacemapper.res");
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
}
