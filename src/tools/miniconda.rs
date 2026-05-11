//! Miniconda installation via shell installer.
//!
//! Downloads and runs the Miniconda installer for the current platform.

use std::fs;
use std::path::Path;
use std::process::Command;

use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use miette::{Context, IntoDiagnostic};
use tokio::io::AsyncWriteExt;

use crate::context::CommandContext;
use crate::paths;
use crate::ui::status;
use crate::ui::styles::UiColor;

const MINICONDA_BASE_URL: &str = "https://repo.anaconda.com/miniconda";

fn installer_filename() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "Miniconda3-latest-MacOSX-x86_64.sh"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "Miniconda3-latest-MacOSX-arm64.sh"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "Miniconda3-latest-Linux-x86_64.sh"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "Miniconda3-latest-Linux-aarch64.sh"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "Miniconda3-latest-Windows-x86_64.exe"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        compile_error!("Unsupported platform for miniconda installation")
    }
}

fn installer_url() -> String {
    format!("{}/{}", MINICONDA_BASE_URL, installer_filename())
}

pub async fn install(ctx: &CommandContext) -> miette::Result<()> {
    let prefix = paths::tool_prefix("miniconda");

    if prefix.exists() {
        return Err(miette::miette!(
            "Miniconda is already installed at {}",
            prefix.display()
        ));
    }

    let url = installer_url();

    let temp_dir = std::env::temp_dir().join(format!("ana-miniconda-{}", std::process::id()));
    fs::create_dir_all(&temp_dir)
        .into_diagnostic()
        .context("failed to create temp directory")?;

    let installer_path = temp_dir.join(installer_filename());

    download_installer(ctx, &url, &installer_path).await?;

    status::blank_line();
    status::info(&format!(
        "Running Miniconda installer (will install to {})...",
        status::highlight(&prefix.display().to_string())
    ));
    status::blank_line();

    let result = run_installer(&installer_path, &prefix);

    let _ = fs::remove_dir_all(&temp_dir);

    result?;

    create_bin_symlinks(&prefix)?;

    status::blank_line();
    status::success("Miniconda installed successfully!");
    status::blank_line();
    status::info("You may need to restart your shell or run:");
    eprintln!(
        "  {}",
        status::highlight("source ~/.ana/tools/miniconda/etc/profile.d/conda.sh")
    );

    Ok(())
}

async fn download_installer(ctx: &CommandContext, url: &str, dest: &Path) -> miette::Result<()> {
    let client = ctx.download_client();
    let response = client
        .get(url)
        .send()
        .await
        .into_diagnostic()
        .context("failed to download installer")?;

    if !response.status().is_success() {
        return Err(miette::miette!(
            "Failed to download installer: HTTP {}",
            response.status()
        ));
    }

    let total_size = response.content_length().unwrap_or(0);
    let total_mb = total_size as f64 / 1_000_000.0;
    let filename = installer_filename();

    eprintln!("  Downloading {} ({:.1} MB)", filename, total_mb);
    eprintln!("  {}", UiColor::Dim.apply_to(url));

    let pb = ProgressBar::new(total_size);
    let dim = UiColor::Dim.hex();
    let dim_suffix = UiColor::Dim.apply_to("% |").to_string();
    let template = format!(
        "  {{bar:34.{}/{dim}}} {{percent:>2.{dim}}}{dim_suffix} {{elapsed:.{dim}}}",
        UiColor::Green.hex(),
    );
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&template)
            .unwrap()
            .progress_chars("━━─"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut file = tokio::fs::File::create(dest)
        .await
        .into_diagnostic()
        .context("failed to create installer file")?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .into_diagnostic()
            .context("failed to read download chunk")?;
        file.write_all(&chunk)
            .await
            .into_diagnostic()
            .context("failed to write installer chunk")?;
        pb.inc(chunk.len() as u64);
    }

    pb.finish_and_clear();

    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprint!("\x1b[2A\x1b[K\x1b[1B\x1b[K\x1b[1A");
    }

    Ok(())
}

#[cfg(unix)]
fn run_installer(installer_path: &Path, prefix: &Path) -> miette::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(installer_path, fs::Permissions::from_mode(0o755))
        .into_diagnostic()
        .context("failed to make installer executable")?;

    let status = Command::new(installer_path)
        .arg("-p")
        .arg(prefix)
        .status()
        .into_diagnostic()
        .context("failed to run installer")?;

    if !status.success() {
        return Err(miette::miette!(
            "Installer exited with code: {}",
            status.code().unwrap_or(-1)
        ));
    }

    Ok(())
}

#[cfg(windows)]
fn run_installer(installer_path: &Path, prefix: &Path) -> miette::Result<()> {
    let status = Command::new(installer_path)
        .args([
            "/InstallationType=JustMe",
            "/AddToPath=0",
            "/RegisterPython=0",
        ])
        .arg(format!("/D={}", prefix.display()))
        .status()
        .into_diagnostic()
        .context("failed to run installer")?;

    if !status.success() {
        return Err(miette::miette!(
            "Installer exited with code: {}",
            status.code().unwrap_or(-1)
        ));
    }

    Ok(())
}

fn create_bin_symlinks(prefix: &Path) -> miette::Result<()> {
    let bin_dir = paths::bin_dir();
    fs::create_dir_all(&bin_dir)
        .into_diagnostic()
        .context("failed to create bin directory")?;

    #[cfg(unix)]
    {
        let conda_bin = prefix.join("bin").join("conda");
        let symlink_path = bin_dir.join("conda");

        if symlink_path.exists() || symlink_path.is_symlink() {
            fs::remove_file(&symlink_path)
                .into_diagnostic()
                .context("failed to remove existing symlink")?;
        }

        std::os::unix::fs::symlink(&conda_bin, &symlink_path)
            .into_diagnostic()
            .with_context(|| format!("failed to create symlink: {}", symlink_path.display()))?;

        eprintln!(
            "   Linked {} -> {}",
            symlink_path.display(),
            conda_bin.display()
        );
    }

    #[cfg(windows)]
    {
        let binary = std::path::PathBuf::from("Scripts").join("conda");
        super::install::create_bin_shim_public(&bin_dir, prefix, &binary)?;
    }

    Ok(())
}

pub fn binaries() -> Vec<std::path::PathBuf> {
    #[cfg(unix)]
    {
        vec![std::path::PathBuf::from("bin/conda")]
    }
    #[cfg(windows)]
    {
        vec![std::path::PathBuf::from("Scripts/conda.exe")]
    }
}
