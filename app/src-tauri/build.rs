fn main() {
  let mut attributes = tauri_build::Attributes::new();
  if target_is_windows_msvc() {
    // tauri-build embeds its manifest only into bin targets, so test binaries
    // would load comctl32 5.82 and fail to resolve TaskDialogIndirect. Embed the
    // Common-Controls v6 manifest through the linker for every target instead.
    attributes =
      attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    let manifest =
      std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
  }
  tauri_build::try_build(attributes).expect("failed to run tauri-build");
}

// Cargo hands a build script its target only through CARGO_CFG_* variables (cfg! here
// describes the host), and amenbo_core::env is not available to a build script, so these
// two Cargo-provided reads are the one place this crate reads the environment directly.
#[allow(clippy::disallowed_methods)]
fn target_is_windows_msvc() -> bool {
  std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
    && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
}
