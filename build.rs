fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Embed assets, but use Windows system fonts at runtime. Rasterizing fonts
    // here registers a process-wide subset containing only the shell's static
    // glyphs; dynamic applets then lose accents and use the wrong font sizes.
    // Runtime SVG decoding is enabled for both shell and applet images.
    slint_build::compile_with_config(
        "ui/shell.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles),
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
