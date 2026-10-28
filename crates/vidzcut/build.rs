//! Puts the vidzcut icon and the version into the Windows program file, so Explorer, the taskbar and the
//! installer show it. (Other systems need nothing here.)

fn main() {
    println!("cargo:rerun-if-changed=../../assets/brand/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("../../assets/brand/icon.ico");
    resource.set("ProductName", "vidzcut");
    resource.set("FileDescription", "vidzcut video editor");
    resource.set("CompanyName", "vidzcut");
    if let Err(e) = resource.compile() {
        // a missing resource compiler must not stop the build: the program just has no icon in Explorer
        println!("cargo:warning=could not add the icon to the program: {e}");
    }
}
