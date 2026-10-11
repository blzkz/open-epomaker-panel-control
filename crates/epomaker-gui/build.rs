//! Embeds the application icon and version information in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/icons/open-epomaker-panel.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/icons/open-epomaker-panel.ico");
        res.set("ProductName", "Open Epomaker Panel");
        res.set("FileDescription", "Open control panel for EPOMAKER keyboards");
        if let Err(e) = res.compile() {
            // a missing resource compiler must not break the build
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
