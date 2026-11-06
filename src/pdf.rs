//! Reports to PDF, without opening a browser window: the system's own
//! Chromium-family browser (Edge on every Windows PC, Chrome or Chromium on
//! most Linux desktops) prints the report page in the background, with the
//! page's own print styles. `ISOBAR_BROWSER` names one to use instead.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Browsers to try, most likely first.
fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("ISOBAR_BROWSER") {
        out.push(PathBuf::from(p));
    }
    #[cfg(windows)]
    {
        let roots: Vec<PathBuf> = ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
            .iter()
            .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
            .collect();
        let apps = [
            r"Microsoft\Edge\Application\msedge.exe",
            r"Google\Chrome\Application\chrome.exe",
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Chromium\Application\chrome.exe",
        ];
        for app in apps {
            for r in &roots {
                out.push(r.join(app));
            }
        }
    }
    #[cfg(not(windows))]
    {
        let names = [
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
            "microsoft-edge-stable",
            "brave-browser",
        ];
        let path = std::env::var_os("PATH").unwrap_or_default();
        for name in names {
            for dir in std::env::split_paths(&path) {
                out.push(dir.join(name));
            }
        }
        out.push(PathBuf::from("/snap/bin/chromium"));
        out.push(PathBuf::from(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        ));
    }
    out
}

/// The first browser that's there.
pub fn browser() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

/// Print `html` (a file) to `pdf`. Both live in `work`, a folder the browser
/// can read and write (a snap or Flatpak browser can't reach hidden or
/// temporary folders). Blocks for up to a minute.
pub fn print(html: &Path, pdf: &Path, work: &Path) -> Result<(), String> {
    let exe = browser().ok_or_else(|| {
        "no Edge, Chrome or Chromium found to make the PDF with. Open the report in your browser and print it to PDF from there".to_string()
    })?;
    // A profile of its own, so a browser that's already open doesn't take
    // the job and quietly drop it.
    let profile = work.join("pdf-profile");
    let _ = std::fs::remove_file(pdf);
    let url = format!(
        "file:///{}",
        html.to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
            .replace(' ', "%20")
            .replace('#', "%23")
    );
    let mut cmd = Command::new(&exe);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--no-pdf-header-footer")
        .arg("--print-to-pdf-no-header")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--print-to-pdf={}", pdf.display()))
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Chromium won't start as root (a container, say) with its sandbox on.
    #[cfg(unix)]
    if std::fs::read_to_string("/proc/self/status").is_ok_and(|s| {
        s.lines()
            .find(|l| l.starts_with("Uid:"))
            .is_some_and(|l| l.split_whitespace().nth(1) == Some("0"))
    }) {
        cmd.arg("--no-sandbox");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("couldn't start {} ({e})", exe.display()))?;
    let start = Instant::now();
    let done = loop {
        match child.try_wait() {
            Ok(Some(_)) => break true,
            Ok(None) if start.elapsed() > Duration::from_secs(60) => {
                let _ = child.kill();
                break false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(_) => break true,
        }
    };
    let _ = std::fs::remove_dir_all(&profile);
    let made = std::fs::metadata(pdf).map(|m| m.len() > 0).unwrap_or(false);
    match (done, made) {
        (_, true) => Ok(()),
        (false, false) => Err(format!(
            "{} took over a minute and was stopped",
            exe.file_name().unwrap_or_default().to_string_lossy()
        )),
        (true, false) => Err(format!(
            "{} didn't write the PDF",
            exe.file_name().unwrap_or_default().to_string_lossy()
        )),
    }
}

#[cfg(test)]
mod tests {
    /// Needs a browser: `ISOBAR_BROWSER=... cargo test -- --ignored pdf`.
    #[test]
    #[ignore]
    fn prints_a_page() {
        let dir = std::env::temp_dir().join(format!("isobar pdf test {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let html = dir.join("A page #1.html");
        std::fs::write(&html, "<h1>Hello</h1><p>Isobar's PDF test.</p>").unwrap();
        let pdf = dir.join("out.pdf");
        super::print(&html, &pdf, &dir).unwrap();
        let bytes = std::fs::read(&pdf).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(!dir.join("pdf-profile").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
