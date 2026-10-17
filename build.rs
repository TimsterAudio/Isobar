//! Gives the Windows .exe its icon.

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/icon.ico");
        res.set("ProductName", "Isobar");
        res.set(
            "FileDescription",
            "Isobar, a desktop companion for Cumulus MX weather stations",
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=The .exe has no icon: {e}");
        }
    }
}
