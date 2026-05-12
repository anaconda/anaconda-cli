//! List available features and their status.

use crate::context::CommandContext;
use crate::ui::status;

/// Information about a feature for display.
pub struct FeatureInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub category: FeatureCategory,
}

/// Category of a feature.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FeatureCategory {
    Stable,
    Experimental,
}

/// List all available features.
pub fn list_features() -> Vec<FeatureInfo> {
    vec![
        FeatureInfo {
            name: "main-x",
            description: "Configure conda/pixi to use Anaconda's main-x channel",
            category: FeatureCategory::Stable,
        },
        FeatureInfo {
            name: "wheels",
            description: "Configure pip/uv to use Anaconda's PyPI mirror",
            category: FeatureCategory::Stable,
        },
        #[cfg(unix)]
        FeatureInfo {
            name: "outerbounds",
            description: "Enable Outerbounds CLI integration (alias: ob)",
            category: FeatureCategory::Experimental,
        },
    ]
}

/// Print a key-value pair with consistent formatting.
fn print_kv(key: &str, value: &str) {
    eprintln!(
        "  {}{}",
        status::dim(&format!("{:<14}", key)),
        status::highlight(value)
    );
}

/// Print the feature list with section headers.
pub fn print_feature_list(_ctx: &mut CommandContext) {
    let features = list_features();

    let stable: Vec<_> = features
        .iter()
        .filter(|f| f.category == FeatureCategory::Stable)
        .collect();
    let experimental: Vec<_> = features
        .iter()
        .filter(|f| f.category == FeatureCategory::Experimental)
        .collect();

    if !stable.is_empty() {
        eprintln!("{}", status::section("stable"));
        for feature in stable {
            print_kv(feature.name, feature.description);
        }
    }

    if !experimental.is_empty() {
        status::blank_line();
        eprintln!("{}", status::section("experimental"));
        for feature in experimental {
            print_kv(feature.name, feature.description);
        }
    }
}
