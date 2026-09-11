fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    winresource::WindowsResource::new()
        .set("ProductName", "Winarchy")
        .set("FileDescription", "Winarchy command-line controller")
        .set("CompanyName", "Yannick Herrero")
        .set("LegalCopyright", "MIT License")
        .set_manifest(include_str!("../../winarchy.manifest"))
        .compile()
        .expect("compile Windows resources");
}
