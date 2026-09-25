fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    slint_build::compile_with_config(
        "ui/apps.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer),
    )
    .expect("compile applications UI");
    winresource::WindowsResource::new()
        .set("ProductName", "Illium")
        .set("FileDescription", "Illium applications")
        .set("CompanyName", "Yannick Herrero")
        .set("LegalCopyright", "MIT License")
        .set_manifest(include_str!("../../illium.manifest"))
        .compile()
        .expect("compile Windows resources");
}
