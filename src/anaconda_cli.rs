use crate::paths;
use crate::tools;

pub fn run_bootstrap() -> Result<(), String> {
    let anaconda_bin = paths::bin_dir().join("anaconda");

    if anaconda_bin.exists() {
        eprintln!("anaconda-cli is already installed");
        return Ok(());
    }

    eprintln!("Installing anaconda-cli...");
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create async runtime: {}", e))?;
    rt.block_on(tools::install::install_tool("anaconda-cli"))
        .map_err(|e| format!("{:?}", e))?;

    eprintln!("anaconda-cli installed successfully");
    Ok(())
}
