fn main() {
    // The resource compiler is available on Windows hosts only. Local checks on other hosts skip it.
    if cfg!(windows) {
        embed_resource::compile("app.rc", embed_resource::NONE)
            .manifest_required()
            .expect("embed the app icon");
    }
}
