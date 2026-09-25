fn main() {
    slint_build::compile("ui/surfaces.slint").unwrap();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Windows 10 awareness also enables layered child HWNDs: the opaque
        // native EDIT needs its own GDI surface above the DComp palette.
        println!("cargo:rerun-if-changed=../../illium.manifest");
        winresource::WindowsResource::new()
            .set("ProductName", "Illium Browser")
            .set("FileDescription", "Illium Browser")
            .set_manifest(include_str!("../../illium.manifest"))
            .compile()
            .expect("compile Windows resources");
    }
}
