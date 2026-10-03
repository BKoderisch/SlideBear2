//! Bettet unter Windows das Eisbär-Icon und Versionsinfos in die `.exe` ein
//! (Explorer, Startmenü, Taskleiste). Auf anderen Systemen passiert nichts.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icon.ico");
    res.set("ProductName", "SlideBear");
    res.set("FileDescription", "SlideBear: Veranstaltungs-Slides aus ChurchTools");
    res.set("CompanyName", "ProKode");
    if let Err(e) = res.compile() {
        println!("cargo:warning=Icon konnte nicht eingebettet werden: {e}");
    }
}
