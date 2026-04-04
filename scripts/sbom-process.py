#!/usr/bin/env python3
"""Process cargo-cyclonedx and cargo-audit output into SBOM.json and SBOM.md.

Usage: sbom-process.py [--force] <sbom-raw.json> <audit.json> <SBOM.json> <SBOM.md>

Reads the raw CycloneDX SBOM from cargo-cyclonedx and vulnerability data from
cargo-audit, merges them, and produces a clean SBOM.json and human-readable
SBOM.md with packages, security advisories, and license summary tables.
"""

import json
import os
import sys
from urllib.parse import unquote

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


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
        # cargo-cyclonedx uses {"expression": "MIT OR Apache-2.0"} format
        if "expression" in entry:
            parts.append(entry["expression"])
        else:
            lic = entry.get("license", {})
            parts.append(lic.get("id", lic.get("name", "NOASSERTION")))
    return " AND ".join(parts) if parts else "NOASSERTION"


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
        # Prefer CVE ID if available
        cve_id = next((a for a in aliases if a.startswith("CVE-")), vuln_id)

        # Build affects list
        affects = []
        pkg_name = package.get("name", "")
        pkg_version = package.get("version", "")
        ref = comp_refs.get((pkg_name, pkg_version), "")
        if ref:
            affects.append({"ref": ref})

        # Build ratings from CVSS if available
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
        # Add RUSTSEC ID as reference if we used CVE as primary
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

    # Add warnings (unmaintained, unsound, etc.) as informational entries
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


def generate_markdown(data: dict) -> str:
    """Generate SBOM.md from a CycloneDX BOM."""
    components = data.get("components", [])
    vulnerabilities = data.get("vulnerabilities", [])
    created = data.get("metadata", {}).get("timestamp", "unknown")
    spec_version = data.get("specVersion", "unknown")

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

    # --- Render ---
    lines: list[str] = []
    lines.append("# Software Bill of Materials (SBOM)")
    lines.append("")
    lines.append(f"Generated: {created}<br>")
    lines.append(f"Format: CycloneDX {spec_version}<br>")
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
    lines.append("## Packages")
    lines.append("")
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

        lines.append(f"| {display_name} | {version} | {license_val} | {cve_display} |")
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
            # Info-level warnings sort last
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
    force = "--force" in sys.argv
    args = [a for a in sys.argv[1:] if a != "--force"]

    if len(args) != 4:
        print(
            f"Usage: {sys.argv[0]} [--force] <sbom-raw.json> <audit.json>"
            f" <SBOM.json> <SBOM.md>",
            file=sys.stderr,
        )
        sys.exit(1)

    raw_path, audit_path, json_path, md_path = args

    with open(raw_path) as f:
        sbom = json.load(f)

    with open(audit_path) as f:
        audit = json.load(f)

    # Merge audit findings into the SBOM
    merge_audit(sbom, audit)

    comp_count = len(sbom.get("components", []))
    vuln_count = len(sbom.get("vulnerabilities", []))
    print(f"==> {comp_count} components, {vuln_count} vulnerabilities")

    # Compare material content against existing SBOM.json
    if not force and os.path.exists(json_path):
        with open(json_path) as f:
            existing = json.load(f)
        if material_content(sbom) == material_content(existing):
            print("==> No material changes — SBOM.json and SBOM.md unchanged")
            return

    # Write clean JSON
    with open(json_path, "w") as f:
        json.dump(sbom, f, indent=2)
        f.write("\n")
    print(f"==> Wrote {json_path}")

    # Generate and write markdown
    md = generate_markdown(sbom)
    with open(md_path, "w") as f:
        f.write(md)
    print(f"==> Wrote {md_path}")


if __name__ == "__main__":
    main()
