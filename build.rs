fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Icons are pre-rendered at build time: the software renderer has no
    // runtime SVG decoder in this feature set.
    slint_build::compile_with_config(
        "ui/shell.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer),
    )
    .expect("compile shell UI");
    winresource::WindowsResource::new()
        .set("ProductName", "Winarchy")
        .set("FileDescription", "Winarchy tiling window manager")
        .set("CompanyName", "Yannick Herrero")
        .set("LegalCopyright", "MIT License")
        .set_manifest(include_str!("winarchy.manifest"))
        .compile()
        .expect("compile Windows resources");
}
