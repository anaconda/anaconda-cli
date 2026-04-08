#!/usr/bin/env python3
"""Process per-target cargo-cyclonedx and cargo-audit output into SBOM.json and SBOM.md.

Usage:
    sbom-process.py [--force] --audit <audit.json>
        --output-json <SBOM.json> --output-md <SBOM.md>
        <target-sbom-1.json> [<target-sbom-2.json> ...]

Merges per-target CycloneDX SBOMs into a single combined SBOM with platform
annotations, integrates cargo-audit vulnerability data, and produces a clean
SBOM.json and human-readable SBOM.md.
"""

import argparse
import json
import os
import re
from urllib.parse import unquote

# Mapping from Rust target triples to short platform labels
TARGET_LABELS: dict[str, str] = {
    "x86_64-unknown-linux-gnu": "linux",
    "aarch64-apple-darwin": "macos",
    "x86_64-pc-windows-msvc": "windows",
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


_LOCAL_PATH_RE = re.compile(r"path\+file:///[^#\"]+(?=#)")


def _sanitize_local_paths(sbom: dict) -> None:
    """Replace absolute local paths with a generic placeholder in-place.

    cargo-cyclonedx embeds the developer's workspace path in bom-ref, purl,
    and dependency ref fields.  We round-trip through JSON so every occurrence
    is caught regardless of where it appears in the tree.
    """
    raw = json.dumps(sbom)
    sanitized = _LOCAL_PATH_RE.sub("path+file:///tmp/ana-cli", raw)
    if sanitized != raw:
        sbom.clear()
        sbom.update(json.loads(sanitized))


def md_escape(text: str) -> str:
    """Escape underscores for Markdown table cells."""
    return text.replace("_", r"\_")


def purl_to_url(purl: str) -> str:
    """Best-effort conversion of a package URL to a browsable registry link."""
    if purl.startswith("pkg:cargo/"):
        name = unquote(purl[len("pkg:cargo/") :].rsplit("@", 1)[0])
        return f"https://crates.io/crates/{name}"
    if purl.startswith("pkg:npm/"):
        rest = purl[len("pkg:npm/") :]
        name = unquote(
            rest.split("@")[-2] if rest.startswith("%40") else rest.rsplit("@", 1)[0]
        )
        if not name.startswith("@"):
            name = unquote(rest.rsplit("@", 1)[0])
        return f"https://www.npmjs.com/package/{name}"
    if purl.startswith("pkg:golang/"):
        module = unquote(purl[len("pkg:golang/") :].rsplit("@", 1)[0])
        return f"https://pkg.go.dev/{module}"
    if purl.startswith("pkg:pypi/"):
        name = unquote(purl[len("pkg:pypi/") :].rsplit("@", 1)[0])
        return f"https://pypi.org/project/{name}/"
    return ""


def get_license(comp: dict) -> str:
    """Extract a license string from a CycloneDX component."""
    licenses = comp.get("licenses", [])
    if not licenses:
        return "NOASSERTION"
    parts = []
    for entry in licenses:
        if "expression" in entry:
            parts.append(entry["expression"])
        else:
            lic = entry.get("license", {})
            parts.append(lic.get("id", lic.get("name", "NOASSERTION")))
    return " AND ".join(parts) if parts else "NOASSERTION"


def label_from_filename(filename: str) -> str:
    """Extract a platform label from a per-target SBOM filename.

    E.g. 'ana-x86_64-unknown-linux-gnu.json' -> 'linux'
    """
    base = os.path.basename(filename)
    for triple, label in TARGET_LABELS.items():
        if triple in base:
            return label
    # Fallback: strip ana- prefix and .json suffix
    return re.sub(r"^ana-|\.json$", "", base)


# ---------------------------------------------------------------------------
# Merging per-target SBOMs
# ---------------------------------------------------------------------------


def merge_target_sboms(
    target_files: list[str],
) -> tuple[dict, dict[tuple[str, str], set[str]]]:
    """Merge multiple per-target CycloneDX SBOMs into a single combined SBOM.

    Returns (combined_sbom, platform_map) where platform_map maps
    (name, version) -> set of platform labels.
    """
    platform_map: dict[tuple[str, str], set[str]] = {}
    # Use the first target as the base SBOM (for metadata, etc.)
    combined: dict = {}
    seen_components: dict[tuple[str, str], dict] = {}
    # Union dependency graph edges across all targets
    seen_deps: dict[str, set[str]] = {}

    for filepath in target_files:
        label = label_from_filename(filepath)
        with open(filepath) as f:
            data = json.load(f)

        if not combined:
            combined = data
        for comp in data.get("components", []):
            key = (comp["name"], comp.get("version", ""))
            platform_map.setdefault(key, set()).add(label)
            if key not in seen_components:
                seen_components[key] = comp
        for dep in data.get("dependencies", []):
            ref = dep["ref"]
            seen_deps.setdefault(ref, set()).update(dep.get("dependsOn", []))

    # Replace components and dependencies with the merged sets
    combined["components"] = list(seen_components.values())
    combined["dependencies"] = [
        {"ref": ref, "dependsOn": sorted(deps)} for ref, deps in seen_deps.items()
    ]

    all_labels = set(TARGET_LABELS.values())

    # Write platform annotations as CycloneDX component properties so that
    # downstream consumers of SBOM.json can see which platforms need each dep.
    for comp in combined["components"]:
        key = (comp["name"], comp.get("version", ""))
        platforms = platform_map.get(key, set())
        if platforms and platforms < all_labels:
            comp.setdefault("properties", []).append(
                {"name": "cdx:ana:platforms", "value": ",".join(sorted(platforms))}
            )

    # Sanitize local filesystem paths (cargo-cyclonedx embeds the developer's
    # absolute path in bom-ref, purl, and dependency ref fields).
    _sanitize_local_paths(combined)

    # Update metadata properties to reflect all merged target triples
    all_triples = sorted(TARGET_LABELS.keys())
    props = combined.get("metadata", {}).get("properties", [])
    for prop in props:
        if prop.get("name") == "cdx:rustc:sbom:target:triple":
            prop["value"] = ",".join(all_triples)
            break

    # Bump specVersion to 1.4 since we use the vulnerabilities field (added in 1.4)
    combined["specVersion"] = "1.4"

    return combined, platform_map


# ---------------------------------------------------------------------------
# Audit merging
# ---------------------------------------------------------------------------


def merge_audit(sbom: dict, audit: dict) -> None:
    """Merge cargo-audit findings into the CycloneDX SBOM as vulnerabilities."""
    vuln_list = audit.get("vulnerabilities", {}).get("list", [])
    warnings = audit.get("warnings", {})

    if not vuln_list and not warnings:
        return

    # Build name+version -> component bom-ref lookup
    comp_refs: dict[tuple[str, str], str] = {}
    for comp in sbom.get("components", []):
        key = (comp["name"], comp.get("version", ""))
        comp_refs[key] = comp.get("bom-ref", "")

    vulnerabilities = sbom.setdefault("vulnerabilities", [])

    for entry in vuln_list:
        advisory = entry.get("advisory", {})
        package = entry.get("package", {})

        vuln_id = advisory.get("id", "")
        aliases = advisory.get("aliases", [])
        cve_id = next((a for a in aliases if a.startswith("CVE-")), vuln_id)

        affects = []
        pkg_name = package.get("name", "")
        pkg_version = package.get("version", "")
        ref = comp_refs.get((pkg_name, pkg_version), "")
        if ref:
            affects.append({"ref": ref})

        ratings = []
        cvss = advisory.get("cvss")
        if cvss:
            score = cvss.get("score")
            if score is not None:
                severity = _cvss_to_severity(float(score))
                ratings.append(
                    {
                        "score": score,
                        "severity": severity.lower(),
                        "method": "CVSSv31",
                    }
                )

        vuln = {
            "id": cve_id,
            "description": advisory.get("description", advisory.get("title", "")),
            "source": {
                "name": "RustSec",
                "url": f"https://rustsec.org/advisories/{vuln_id}.html",
            },
            "ratings": ratings,
            "affects": affects,
        }
        if cve_id != vuln_id:
            vuln["references"] = [
                {
                    "id": vuln_id,
                    "source": {
                        "name": "RustSec",
                        "url": f"https://rustsec.org/advisories/{vuln_id}.html",
                    },
                }
            ]

        vulnerabilities.append(vuln)

    for warn_type, warn_list in warnings.items():
        for warn_entry in warn_list:
            advisory = warn_entry.get("advisory", {})
            package = warn_entry.get("package", {})
            vuln_id = advisory.get("id", "")

            affects = []
            pkg_name = package.get("name", "")
            pkg_version = package.get("version", "")
            ref = comp_refs.get((pkg_name, pkg_version), "")
            if ref:
                affects.append({"ref": ref})

            vulnerabilities.append(
                {
                    "id": vuln_id,
                    "description": f"[{warn_type}] {advisory.get('title', '')}",
                    "source": {
                        "name": "RustSec",
                        "url": f"https://rustsec.org/advisories/{vuln_id}.html",
                    },
                    "ratings": [{"severity": "info", "method": "other"}],
                    "affects": affects,
                }
            )


def _cvss_to_severity(score: float) -> str:
    """Convert a CVSS v3 score to a severity string."""
    if score >= 9.0:
        return "CRITICAL"
    if score >= 7.0:
        return "HIGH"
    if score >= 4.0:
        return "MEDIUM"
    if score > 0.0:
        return "LOW"
    return "NONE"


# ---------------------------------------------------------------------------
# Markdown generation
# ---------------------------------------------------------------------------


def extract_scores(vuln: dict) -> tuple[str, str, str]:
    """Extract (cvss_v2, cvss_v3, severity) from a CycloneDX vulnerability."""
    v2 = ""
    v3 = ""
    severity = ""
    for rating in vuln.get("ratings", []):
        method = rating.get("method", "")
        score = rating.get("score")
        sev = rating.get("severity", "")
        if method == "CVSSv2" and score is not None:
            v2 = str(score)
        elif method in ("CVSSv30", "CVSSv31", "CVSSv40") and score is not None:
            v3 = str(score)
            if sev:
                severity = sev.upper()
        elif method == "other" and sev:
            severity = sev.upper()
    if not severity:
        for rating in vuln.get("ratings", []):
            if rating.get("severity"):
                severity = rating["severity"].upper()
                break
    return v2, v3, severity


def platform_display(platforms: set[str], all_labels: set[str]) -> str:
    """Return a display string for platform annotations.

    Returns empty string if the dep is on all platforms (no annotation needed).
    """
    if platforms >= all_labels:
        return ""
    return ", ".join(sorted(platforms))


def generate_markdown(
    data: dict,
    platform_map: dict[tuple[str, str], set[str]] | None = None,
) -> str:
    """Generate SBOM.md from a CycloneDX BOM with optional platform annotations."""
    components = data.get("components", [])
    vulnerabilities = data.get("vulnerabilities", [])
    created = data.get("metadata", {}).get("timestamp", "unknown")
    spec_version = data.get("specVersion", "unknown")

    all_labels = set()
    if platform_map:
        for platforms in platform_map.values():
            all_labels |= platforms

    # Build bom-ref -> component lookup
    ref_to_comp: dict[str, dict] = {}
    for comp in components:
        ref = comp.get("bom-ref", "")
        if ref:
            ref_to_comp[ref] = comp

    # Map (name_lower, version) -> list of vulns
    comp_vulns: dict[tuple[str, str], list[dict]] = {}
    for vuln in vulnerabilities:
        for affect in vuln.get("affects", []):
            comp = ref_to_comp.get(affect.get("ref", ""))
            if comp:
                key = (comp["name"].lower(), comp.get("version", ""))
                comp_vulns.setdefault(key, []).append(vuln)

    components_sorted = sorted(components, key=lambda c: c.get("name", "").lower())

    # License summary
    license_counts: dict[str, int] = {}
    for comp in components_sorted:
        lic = get_license(comp)
        license_counts[lic] = license_counts.get(lic, 0) + 1

    # Advisory rows (deduplicated by name+version+id)
    advisory_rows: list[tuple[str, str, str, str, str, str]] = []
    seen_advisories: set[str] = set()
    for comp in components_sorted:
        name = comp.get("name", "")
        version = comp.get("version", "")
        for vuln in comp_vulns.get((name.lower(), version), []):
            vuln_id = vuln.get("id", "")
            dedup_key = f"{name.lower()}@{version}:{vuln_id}"
            if dedup_key in seen_advisories:
                continue
            seen_advisories.add(dedup_key)
            v2, v3, severity = extract_scores(vuln)
            advisory_rows.append((name, version, vuln_id, v2, v3, severity))

    affected_pkgs = set(f"{r[0]}@{r[1]}" for r in advisory_rows)

    # Count platform-specific deps
    platform_specific_count = 0
    if platform_map and all_labels:
        for comp in components:
            key = (comp["name"], comp.get("version", ""))
            platforms = platform_map.get(key, all_labels)
            if platforms < all_labels:
                platform_specific_count += 1

    # --- Render ---
    lines: list[str] = []
    lines.append("# Software Bill of Materials (SBOM)")
    lines.append("")
    lines.append(f"Generated: {created}<br>")
    lines.append(f"Format: CycloneDX {spec_version}<br>")
    if platform_specific_count:
        lines.append(
            f"Packages: {len(components)}"
            f" ({platform_specific_count} platform-specific)<br>"
        )
        lines.append(f"Platforms: {', '.join(sorted(all_labels))}")
    else:
        lines.append(f"Packages: {len(components)}")
    if not advisory_rows:
        lines.append("<br>**Security advisories: 0 found at this time**")
    else:
        sev_counts: dict[str, int] = {}
        for _, _, _, _, _, sev in advisory_rows:
            if sev:
                sev_counts[sev] = sev_counts.get(sev, 0) + 1
        sev_parts = []
        for level in ("CRITICAL", "HIGH", "MEDIUM", "LOW", "INFO"):
            if level in sev_counts:
                sev_parts.append(f"{sev_counts[level]} {level}")
        sev_summary = f" ({', '.join(sev_parts)})" if sev_parts else ""
        pkg_word = "package" if len(affected_pkgs) == 1 else "packages"
        lines.append(
            f"<br>**[Security advisories](#security-advisories):"
            f" {len(advisory_rows)}{sev_summary}"
            f" across {len(affected_pkgs)} {pkg_word}**"
        )
    lines.append("")

    # Package table
    has_platforms = platform_map is not None and all_labels
    lines.append("## Packages")
    lines.append("")
    if has_platforms:
        lines.append("| Package | Version | License | Platforms | CVEs |")
        lines.append("| --- | --- | --- | --- | ---: |")
    else:
        lines.append("| Package | Version | License | CVEs |")
        lines.append("| --- | --- | --- | ---: |")
    for comp in components_sorted:
        name = comp.get("name", "")
        version = comp.get("version", "")
        license_val = get_license(comp)

        purl = comp.get("purl", "")
        link = purl_to_url(purl)
        escaped_name = md_escape(name)
        display_name = f"[{escaped_name}]({link})" if link else escaped_name

        cve_count = sum(
            1 for r in advisory_rows if r[0].lower() == name.lower() and r[1] == version
        )
        cve_display = str(cve_count) if cve_count else ""

        if has_platforms:
            key = (name, version)
            platforms = platform_map.get(key, all_labels)
            plat_display = platform_display(platforms, all_labels)
            lines.append(
                f"| {display_name} | {version} | {license_val}"
                f" | {plat_display} | {cve_display} |"
            )
        else:
            lines.append(
                f"| {display_name} | {version} | {license_val} | {cve_display} |"
            )
    lines.append("")

    # Security advisories table
    if advisory_rows:
        lines.append("## Security Advisories")
        lines.append("")
        lines.append("| Package | Version | Advisory | CVSS v2 | CVSS v3 | Severity |")
        lines.append("| --- | --- | --- | :---: | :---: | --- |")

        def _sort_key(row):
            _, _, _, v2, v3, sev = row
            v2_f = float(v2) if v2 else 0.0
            v3_f = float(v3) if v3 else 0.0
            info_penalty = 100 if sev == "INFO" else 0
            return (info_penalty, -max(v3_f, v2_f), -v3_f, -v2_f, row[0].lower())

        for pkg_name, version, vuln_id, v2, v3, severity in sorted(
            advisory_rows, key=_sort_key
        ):
            if vuln_id.startswith("CVE-"):
                url = f"https://nvd.nist.gov/vuln/detail/{vuln_id}"
            elif vuln_id.startswith("RUSTSEC-"):
                url = f"https://rustsec.org/advisories/{vuln_id}.html"
            else:
                url = ""
            id_display = f"[{vuln_id}]({url})" if url else vuln_id
            lines.append(
                f"| {md_escape(pkg_name)} | {version} | {id_display}"
                f" | {v2} | {v3} | {severity} |"
            )
        lines.append("")

    # License summary
    lines.append("## License Summary")
    lines.append("")
    lines.append("| License | Count |")
    lines.append("| --- | ---: |")
    for lic, count in sorted(license_counts.items(), key=lambda x: (-x[1], x[0])):
        lines.append(f"| {lic} | {count} |")
    lines.append("")

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def _strip_volatile(comp: dict) -> dict:
    """Return a copy of a component with run-specific fields removed."""
    return {k: v for k, v in comp.items() if k != "bom-ref"}


def material_content(data: dict) -> tuple[list, list]:
    """Extract the material (non-metadata) content for comparison.

    Strips bom-ref fields since they may change between runs.
    """
    comps = [_strip_volatile(c) for c in data.get("components", [])]
    vulns = data.get("vulnerabilities", [])
    return (comps, vulns)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Merge per-target CycloneDX SBOMs with cargo-audit data"
    )
    parser.add_argument("--force", action="store_true", help="Force regeneration")
    parser.add_argument("--audit", required=True, help="cargo-audit JSON output")
    parser.add_argument("--output-json", required=True, help="Output SBOM.json path")
    parser.add_argument("--output-md", required=True, help="Output SBOM.md path")
    parser.add_argument(
        "target_sboms", nargs="+", help="Per-target CycloneDX SBOM JSON files"
    )
    args = parser.parse_args()

    # Merge per-target SBOMs
    sbom, platform_map = merge_target_sboms(args.target_sboms)

    with open(args.audit) as f:
        audit = json.load(f)

    # Merge audit findings into the SBOM
    merge_audit(sbom, audit)

    comp_count = len(sbom.get("components", []))
    vuln_count = len(sbom.get("vulnerabilities", []))
    platform_specific = sum(
        1
        for platforms in platform_map.values()
        if platforms < set(TARGET_LABELS.values())
    )
    print(
        f"==> {comp_count} components ({platform_specific} platform-specific),"
        f" {vuln_count} vulnerabilities"
    )

    # Compare material content against existing SBOM.json
    if not args.force and os.path.exists(args.output_json):
        with open(args.output_json) as f:
            existing = json.load(f)
        if material_content(sbom) == material_content(existing):
            print("==> No material changes — SBOM.json and SBOM.md unchanged")
            return

    # Write clean JSON
    with open(args.output_json, "w") as f:
        json.dump(sbom, f, indent=2)
        f.write("\n")
    print(f"==> Wrote {args.output_json}")

    # Generate and write markdown
    md = generate_markdown(sbom, platform_map)
    with open(args.output_md, "w") as f:
        f.write(md)
    print(f"==> Wrote {args.output_md}")


if __name__ == "__main__":
    main()
