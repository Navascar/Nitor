//! Іконка й опис nitor.exe (на Windows).

fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        res.set("FileDescription", "Nitor — керування RGB-прожекторами");
        res.set("ProductName", "Nitor");
        let _ = res.compile();
    }
}
