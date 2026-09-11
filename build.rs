fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        slint_build::compile("ui/shell.slint").expect("compile shell UI");
    }
}
