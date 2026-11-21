fn main() {
    // Tauri embeds `frontendDist` into the binary. Cargo only knows about the
    // directory path, so a rebuild of the frontend alone would leave a stale
    // bundle inside the executable. Hash the asset contents and emit them as
    // rerun triggers so `cargo build` always picks up a fresh `npm run build`.
    let dist = std::path::Path::new("../dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    if let Ok(entries) = std::fs::read_dir(dist) {
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            println!("cargo:rerun-if-changed={}", path.display());
            if path.is_dir() {
                if let Ok(children) = std::fs::read_dir(&path) {
                    let mut inner: Vec<_> = children.flatten().map(|e| e.path()).collect();
                    inner.sort();
                    for child in inner {
                        println!("cargo:rerun-if-changed={}", child.display());
                    }
                }
            }
        }
    }

    tauri_build::build()
}
