//! Embeds `kagari-desktop-runtime.rc` (the Kagari logo as the executable's icon, resource 1)
//! into the Windows executable. Other targets get nothing.

fn main() {
    println!("cargo::rerun-if-changed=kagari-desktop-runtime.rc");
    println!("cargo::rerun-if-changed=resources/kagari.ico");
    // Fails a Windows build that cannot compile the resource rather than ship it without its icon.
    if let Err(error) = embed_resource::compile("kagari-desktop-runtime.rc", embed_resource::NONE)
        .manifest_required()
    {
        println!("cargo::error={error}");
    }
}
