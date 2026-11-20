//! Вкладає в інсталятор стиснену програму (`NITOR_APP_EXE` — шлях до зібраного nitor.exe)
//! і іконку. Без `NITOR_APP_EXE` інсталятор збирається порожнім (для перевірок і тестів).

use std::io::Write;

fn main() {
    println!("cargo:rerun-if-env-changed=NITOR_APP_EXE");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("payload.bin");
    let data = match std::env::var("NITOR_APP_EXE") {
        Ok(p) if !p.is_empty() => {
            // відносний шлях — від кореня workspace
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let p = root.join(&p).display().to_string();
            println!("cargo:rerun-if-changed={p}");
            std::fs::read(&p).unwrap_or_else(|e| panic!("не вдалося прочитати {p}: {e}"))
        }
        _ => Vec::new(),
    };
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(&data).unwrap();
    std::fs::write(&out, enc.finish().unwrap()).unwrap();

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        res.set("FileDescription", "Встановлення Nitor");
        res.set("ProductName", "Nitor");
        let _ = res.compile();
    }
}
