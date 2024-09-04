#[cfg(target_os = "linux")]
mod main_linux;
#[cfg(target_os = "macos")]
mod main_mac;

mod game;

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        main_mac::main().await
    }
    #[cfg(target_os = "linux")]
    {
        main_linux::main().await
    }
}
