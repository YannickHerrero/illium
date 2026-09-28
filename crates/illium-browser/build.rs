fn main() {
    slint_build::compile("ui/surfaces.slint").unwrap();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Windows 10 awareness also enables layered child HWNDs: the native
        // EDIT stays focusable at alpha zero while Slint mirrors its contents.
        println!("cargo:rerun-if-changed=../../illium.manifest");
        winresource::WindowsResource::new()
            .set("ProductName", "Illium Browser")
            .set("FileDescription", "Illium Browser")
            .set_manifest(include_str!("../../illium.manifest"))
            .compile()
            .expect("compile Windows resources");
    }
}
