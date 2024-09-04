#[cfg(target_os = "macos")]
fn main() {
    let vk_sdk_base = std::path::PathBuf::from(env!("VULKAN_SDK"))
        .parent()
        .expect("Failed to calc parent for VULKAN_SDK")
        .to_path_buf();
    let framework_path = vk_sdk_base.join("macOS/Frameworks");

    println!(
        "cargo:rustc-link-search=framework={}",
        framework_path.display()
    );
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        framework_path.display()
    );
    // println!("cargo:rustc-link-lib=c++");
    // println!("cargo:rustc-link-lib=framework=IOSurface");
    // println!("cargo:rustc-link-lib=framework=IOKit");
}

#[cfg(target_os = "linux")]
fn main() {}
