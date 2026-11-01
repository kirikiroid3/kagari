//! Embeds `kagari-desktop-runtime.rc` (the Kagari logo as the executable's icon, resource 1)
//! into the Windows executable. On Linux, the binary looks for shared libraries (the shipped
//! FFmpeg) in its own directory first.

fn main() {
    println!("cargo::rerun-if-changed=kagari-desktop-runtime.rc");
    println!("cargo::rerun-if-changed=resources/kagari.ico");
    // Fails a Windows build that cannot compile the resource rather than ship it without its icon.
    if let Err(error) = embed_resource::compile("kagari-desktop-runtime.rc", embed_resource::NONE)
        .manifest_required()
    {
        println!("cargo::error={error}");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
}
