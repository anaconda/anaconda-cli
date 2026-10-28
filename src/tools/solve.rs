//! Install-time environment solving for tools that float with their channel
//! instead of being pinned by an embedded lockfile.

use std::time::Instant;

use miette::{Context, IntoDiagnostic};
use rattler_conda_types::{
    Channel, ChannelConfig, GenericVirtualPackage, MatchSpec, ParseStrictness, Platform,
    PrefixRecord, RepoDataRecord,
};
use rattler_repodata_gateway::{Gateway, IndicatifReporter};
use rattler_solve::{ChannelPriority, SolverImpl, SolverTask, resolvo};
use rattler_virtual_packages::{VirtualPackage, VirtualPackageOverrides};

use super::install::{MULTI_PROGRESS, rattler_client, rattler_cache_dir};

/// Resolve `packages` against `channels` for the current platform.
///
/// `packages` always resolve to the newest compatible build. Other packages
/// already in `installed` are preferred when still compatible, so a tool
/// update does not churn unrelated dependencies. Channels use strict
/// priority: a package found in an earlier channel is never taken from a
/// later one.
pub async fn solve(
    channels: &[String],
    packages: &[&str],
    installed: &[PrefixRecord],
) -> miette::Result<Vec<RepoDataRecord>> {
    // Tool channels are absolute URLs; the root only resolves relative names.
    let channel_config = ChannelConfig::default_with_root_dir(crate::paths::ana_home());
    let channels = channels
        .iter()
        .map(|c| {
            Channel::from_str(c, &channel_config)
                .into_diagnostic()
                .with_context(|| format!("invalid channel: {c}"))
        })
        .collect::<miette::Result<Vec<_>>>()?;
    let specs = packages
        .iter()
        .map(|p| {
            MatchSpec::from_str(p, ParseStrictness::Strict)
                .into_diagnostic()
                .with_context(|| format!("invalid package spec: {p}"))
        })
        .collect::<miette::Result<Vec<_>>>()?;

    let gateway = Gateway::builder()
        .with_client(rattler_client()?)
        .with_cache_dir(rattler_cache_dir()?)
        .finish();

    let start = Instant::now();
    let repodata = gateway
        .query(
            channels,
            [Platform::current(), Platform::NoArch],
            specs.clone(),
        )
        .recursive(true)
        .with_reporter(
            IndicatifReporter::builder()
                .with_multi_progress(MULTI_PROGRESS.clone())
                .finish(),
        )
        .await
        .into_diagnostic()
        .context("failed to fetch channel repodata")?
        .repodata;

    let virtual_packages = VirtualPackage::detect(&VirtualPackageOverrides::from_env())
        .into_diagnostic()
        .context("failed to detect system virtual packages")?
        .into_iter()
        .map(GenericVirtualPackage::from)
        .collect();

    let task = SolverTask {
        locked_packages: preferred_records(installed, packages),
        virtual_packages,
        specs,
        channel_priority: ChannelPriority::Strict,
        ..SolverTask::from_iter(&repodata)
    };
    let records = resolvo::Solver
        .solve(task)
        .into_diagnostic()
        .context("failed to solve tool environment")?
        .records;

    eprintln!(
        "   Solved {} packages for {} in {:.1}s",
        records.len(),
        Platform::current(),
        start.elapsed().as_secs_f64()
    );
    Ok(records)
}

/// Installed records the solver should prefer to keep.
///
/// The requested `packages` are excluded: resolvo treats locked records as
/// the preferred candidate, which would otherwise pin the tool itself to its
/// installed version and make updates impossible.
fn preferred_records<'a>(
    installed: &'a [PrefixRecord],
    packages: &[&str],
) -> Vec<&'a RepoDataRecord> {
    installed
        .iter()
        .map(|p| &p.repodata_record)
        .filter(|r| !packages.contains(&r.package_record.name.as_normalized()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rattler_conda_types::package::DistArchiveIdentifier;
    use rattler_conda_types::{PackageName, PackageRecord, Version};

    fn installed_record(name: &str) -> PrefixRecord {
        let repodata_record = RepoDataRecord {
            package_record: PackageRecord::new(
                PackageName::new_unchecked(name),
                Version::major(1),
                "0".to_string(),
            ),
            identifier: DistArchiveIdentifier::try_from_filename(&format!("{name}-1-0.conda"))
                .unwrap(),
            url: url::Url::parse("file:///dev/null").unwrap(),
            channel: None,
        };
        PrefixRecord::from_repodata_record(repodata_record, Vec::new())
    }

    #[test]
    fn test_preferred_records_excludes_floating_packages() {
        let installed = [installed_record("kilo-cli"), installed_record("ripgrep")];
        let names: Vec<_> = preferred_records(&installed, &["kilo-cli"])
            .iter()
            .map(|r| r.package_record.name.as_normalized().to_string())
            .collect();
        assert_eq!(names, ["ripgrep"]);
    }
}
