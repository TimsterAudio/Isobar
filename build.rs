//! Gives the Windows .exe its icon, its file details (what Explorer shows
//! under Properties → Details) and an application manifest. Programs without
//! these look more suspicious to antivirus heuristics.

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="TimsterAudio.Isobar" version="0.0.0.0" processorArchitecture="*"/>
  <description>Isobar, a desktop companion for Cumulus MX weather stations</description>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/icon.ico");
        // FileVersion and ProductVersion come from Cargo.toml.
        res.set("ProductName", "Isobar");
        res.set(
            "FileDescription",
            "Isobar, a desktop companion for Cumulus MX weather stations",
        );
        res.set("CompanyName", "TimsterAudio");
        res.set("LegalCopyright", "© 2026 TimsterAudio");
        res.set("OriginalFilename", "isobar.exe");
        res.set("InternalName", "isobar");
        res.set(
            "Comments",
            "Built by GitHub Actions from https://github.com/TimsterAudio/Isobar",
        );
        res.set_manifest(MANIFEST);
        if let Err(e) = res.compile() {
            println!("cargo:warning=The .exe has no icon or file details: {e}");
        }
    }
}
