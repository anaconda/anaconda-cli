#!/usr/bin/env python3
"""
Create a Jira release for ana-cli and generate QA testing notes.

Two-step workflow:
    1. Generate notes for review:
       python scripts/create_jira_release.py v0.0.9 --generate-notes > qa_notes.md

    2. Create release with reviewed notes:
       python scripts/create_jira_release.py v0.0.9 --notes-file qa_notes.md
       # or via stdin:
       cat qa_notes.md | python scripts/create_jira_release.py v0.0.9 --notes-stdin

Required environment variables (for step 2 only):
    ATLASSIAN_USER_EMAIL - Your Atlassian account email
    ATLASSIAN_API_TOKEN  - API token from https://id.atlassian.com/manage-profile/security/api-tokens

You can set these in a .env file and source it before running.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from dataclasses import field
from datetime import date
from datetime import datetime
from typing import Any

import requests

JIRA_BASE_URL = "https://anaconda.atlassian.net"
JIRA_PROJECT_KEY = "CLI"
JIRA_PROJECT_ID = 11160
GITHUB_REPO = "anaconda/ana-cli"


@dataclass
class PullRequest:
    number: int
    title: str
    body: str
    author: str
    jira_link: str | None
    is_bot: bool

    @property
    def url(self) -> str:
        return f"https://github.com/{GITHUB_REPO}/pull/{self.number}"

    @property
    def category(self) -> str:
        title_lower = self.title.lower()
        if title_lower.startswith("feat"):
            return "feature"
        elif title_lower.startswith("fix"):
            return "fix"
        elif title_lower.startswith(("refac", "chore", "docs", "test", "ci")):
            return "maintenance"
        return "other"


@dataclass
class ReleaseInfo:
    version: str
    github_url: str
    published_at: date
    prs: list[PullRequest] = field(default_factory=list)

    @property
    def version_name(self) -> str:
        return f"ana-cli {self.version}"

    @property
    def jira_issues(self) -> list[str]:
        return [pr.jira_link for pr in self.prs if pr.jira_link]


class JiraClient:
    def __init__(self, email: str, api_token: str):
        self.auth = (email, api_token)
        self.base_url = JIRA_BASE_URL

    def _request(
        self, method: str, endpoint: str, data: dict | None = None
    ) -> dict | None:
        url = f"{self.base_url}/rest/api/3{endpoint}"
        headers = {"Content-Type": "application/json"}

        response = requests.request(
            method,
            url,
            auth=self.auth,
            headers=headers,
            json=data,
        )

        if response.status_code == 204:
            return None

        if not response.ok:
            raise RuntimeError(
                f"Jira API error ({response.status_code}): {response.text}"
            )

        return response.json()

    def get_versions(self) -> list[dict]:
        result = self._request("GET", f"/project/{JIRA_PROJECT_KEY}/versions")
        return result or []

    def version_exists(self, name: str) -> bool:
        return any(v["name"] == name for v in self.get_versions())

    def create_version(self, name: str, description: str, release_date: str) -> dict:
        return self._request(
            "POST",
            "/version",
            {
                "name": name,
                "description": description,
                "projectId": JIRA_PROJECT_ID,
                "released": True,
                "releaseDate": release_date,
            },
        )

    def update_issue_fix_version(self, issue_key: str, version_id: str) -> None:
        self._request(
            "PUT",
            f"/issue/{issue_key}",
            {"fields": {"fixVersions": [{"id": version_id}]}},
        )

    def create_issue(
        self,
        summary: str,
        description: dict,
        issue_type_id: str = "10003",  # Story
        fix_version_id: str | None = None,
    ) -> dict:
        fields = {
            "project": {"key": JIRA_PROJECT_KEY},
            "issuetype": {"id": issue_type_id},
            "summary": summary,
            "description": description,
        }
        if fix_version_id:
            fields["fixVersions"] = [{"id": fix_version_id}]

        return self._request("POST", "/issue", {"fields": fields})


def gh(*args: str) -> str:
    """Run a GitHub CLI command and return stdout."""
    result = subprocess.run(
        ["gh", *args],
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout


def get_github_release(version: str) -> dict:
    output = gh(
        "release",
        "view",
        version,
        "--repo",
        GITHUB_REPO,
        "--json",
        "tagName,body,url,publishedAt",
    )
    return json.loads(output)


def get_pr_details(pr_number: int) -> dict:
    output = gh(
        "pr",
        "view",
        str(pr_number),
        "--repo",
        GITHUB_REPO,
        "--json",
        "title,body,author",
    )
    return json.loads(output)


def extract_pr_numbers(release_body: str) -> list[int]:
    """Extract PR numbers from release notes body."""
    matches = re.findall(r"pull/(\d+)", release_body)
    return sorted(set(int(m) for m in matches))


def extract_jira_link(pr_body: str) -> str | None:
    """Extract Jira issue key from PR body."""
    if not pr_body:
        return None
    match = re.search(r"CLI-\d+", pr_body)
    return match.group(0) if match else None


def fetch_release_info(version: str, quiet: bool = False) -> ReleaseInfo:
    """Fetch GitHub release and PR details."""
    if not quiet:
        print(f"Fetching GitHub release {version}...", file=sys.stderr)
    release = get_github_release(version)
    github_url = release["url"]
    published_at = datetime.fromisoformat(
        release["publishedAt"].replace("Z", "+00:00")
    ).date()

    if not quiet:
        print("Extracting PRs from release notes...", file=sys.stderr)
    pr_numbers = extract_pr_numbers(release["body"])

    prs = []
    for num in pr_numbers:
        if not quiet:
            print(f"  Fetching PR #{num}...", file=sys.stderr)
        try:
            details = get_pr_details(num)
            author = details.get("author", {}).get("login", "unknown")
            is_bot = "renovate" in author.lower() or "bot" in author.lower()

            prs.append(
                PullRequest(
                    number=num,
                    title=details.get("title", f"PR #{num}"),
                    body=details.get("body", ""),
                    author=author,
                    jira_link=extract_jira_link(details.get("body", "")),
                    is_bot=is_bot,
                )
            )
        except subprocess.CalledProcessError:
            if not quiet:
                print(f"    Warning: Could not fetch PR #{num}", file=sys.stderr)

    return ReleaseInfo(
        version=version,
        github_url=github_url,
        published_at=published_at,
        prs=prs,
    )


def generate_notes_markdown(release: ReleaseInfo) -> str:
    """Generate markdown QA notes for human review."""
    lines = []
    lines.append(f"# QA Testing Notes for {release.version_name}")
    lines.append("")
    lines.append(f"GitHub Release: {release.github_url}")
    lines.append("")

    non_bot_prs = [pr for pr in release.prs if not pr.is_bot]
    features = [pr for pr in non_bot_prs if pr.category == "feature"]
    fixes = [pr for pr in non_bot_prs if pr.category == "fix"]
    maintenance = [pr for pr in non_bot_prs if pr.category == "maintenance"]
    other = [pr for pr in non_bot_prs if pr.category == "other"]

    def write_pr_section(prs: list[PullRequest], header: str) -> None:
        if not prs:
            return
        lines.append(f"## {header}")
        lines.append("")
        for pr in prs:
            lines.append(f"### [PR #{pr.number}]({pr.url}) - {pr.title}")
            lines.append("")
            lines.append("**Testing notes:**")
            lines.append("")
            lines.append(
                "<!-- Synthesize testing guidance from the PR description below -->"
            )
            lines.append(
                "<!-- Delete the PR description after writing testing notes -->"
            )
            lines.append("")
            if pr.body:
                for body_line in pr.body.split("\n"):
                    lines.append(f"> {body_line}")
            else:
                lines.append("> (No PR description)")
            lines.append("")

    write_pr_section(features, "Features")
    write_pr_section(fixes, "Bug Fixes")
    write_pr_section(maintenance, "Maintenance (no user-facing testing needed)")
    write_pr_section(other, "Other Changes")

    if release.jira_issues:
        lines.append("## Linked Jira Issues")
        lines.append("")
        lines.append("The following issues will have their Fix Version updated:")
        lines.append("")
        for issue in release.jira_issues:
            lines.append(f"- [{issue}]({JIRA_BASE_URL}/browse/{issue})")
        lines.append("")

    return "\n".join(lines)


def parse_notes_markdown(content: str) -> tuple[str, dict[str, str]]:
    """
    Parse reviewed markdown notes.

    Returns:
        tuple of (github_url, {pr_number: testing_notes})
    """
    github_url = ""
    pr_notes: dict[str, str] = {}

    # Extract GitHub URL
    url_match = re.search(r"GitHub Release: (https://[^\s]+)", content)
    if url_match:
        github_url = url_match.group(1)

    # Extract PR sections - look for ### [PR #N] headers followed by testing notes
    pr_pattern = re.compile(
        r"### \[PR #(\d+)\][^\n]*\n"
        r".*?\*\*Testing notes:\*\*\s*\n"
        r"(.*?)"
        r"(?=### \[PR #|\n## |$)",
        re.DOTALL,
    )

    for match in pr_pattern.finditer(content):
        pr_num = match.group(1)
        notes_section = match.group(2).strip()

        # Remove HTML comments and quoted PR description
        lines = []
        for line in notes_section.split("\n"):
            line = line.strip()
            if line.startswith("<!--") or line.startswith(">"):
                continue
            if line:
                lines.append(line)

        if lines:
            pr_notes[pr_num] = "\n".join(lines)

    return github_url, pr_notes


def build_adf_from_notes(
    release: ReleaseInfo, pr_notes: dict[str, str]
) -> dict[str, Any]:
    """Build Atlassian Document Format from reviewed notes."""
    content: list[dict[str, Any]] = []

    # Header
    content.append(heading("Release Info", level=2))
    content.append(
        paragraph(
            text("GitHub Release: "),
            link("Release Notes", release.github_url),
        )
    )

    non_bot_prs = [pr for pr in release.prs if not pr.is_bot]
    features = [pr for pr in non_bot_prs if pr.category == "feature"]
    fixes = [pr for pr in non_bot_prs if pr.category == "fix"]
    maintenance = [pr for pr in non_bot_prs if pr.category == "maintenance"]
    other = [pr for pr in non_bot_prs if pr.category == "other"]

    def build_pr_section(prs: list[PullRequest], header: str) -> None:
        if not prs:
            return
        content.append(heading(header, level=2))

        items = []
        for pr in prs:
            pr_num_str = str(pr.number)
            notes = pr_notes.get(pr_num_str, "")

            item_content = [
                paragraph(
                    link(f"PR #{pr.number}", pr.url, bold=True),
                    text(f" - {pr.title}"),
                )
            ]

            if notes:
                # Add testing notes as sub-content
                for note_line in notes.split("\n"):
                    if note_line.strip():
                        item_content.append(paragraph(text(note_line)))

            items.append(list_item(*item_content))

        content.append(bullet_list(items))

    build_pr_section(features, "Features")
    build_pr_section(fixes, "Bug Fixes")
    build_pr_section(maintenance, "Maintenance")
    build_pr_section(other, "Other Changes")

    return {"type": "doc", "version": 1, "content": content}


# ADF helper functions
def text(value: str, bold: bool = False, code: bool = False) -> dict:
    node: dict[str, Any] = {"type": "text", "text": value}
    marks: list[dict[str, Any]] = []
    if bold:
        marks.append({"type": "strong"})
    if code:
        marks.append({"type": "code"})
    if marks:
        node["marks"] = marks
    return node


def link(label: str, url: str, bold: bool = False) -> dict:
    marks: list[dict[str, Any]] = [{"type": "link", "attrs": {"href": url}}]
    if bold:
        marks.append({"type": "strong"})
    return {"type": "text", "text": label, "marks": marks}


def paragraph(*nodes: dict) -> dict:
    return {"type": "paragraph", "content": list(nodes)}


def heading(value: str, level: int = 1) -> dict:
    return {
        "type": "heading",
        "attrs": {"level": level},
        "content": [text(value)],
    }


def list_item(*nodes: dict) -> dict:
    return {"type": "listItem", "content": list(nodes)}


def bullet_list(items: list[dict]) -> dict:
    return {"type": "bulletList", "content": items}


def cmd_generate_notes(args: argparse.Namespace) -> int:
    """Generate QA notes markdown for review."""
    release = fetch_release_info(args.version)
    print(generate_notes_markdown(release))
    return 0


def cmd_create_release(args: argparse.Namespace) -> int:
    """Create Jira release with reviewed notes."""
    # Check environment
    email = os.environ.get("ATLASSIAN_USER_EMAIL")
    token = os.environ.get("ATLASSIAN_API_TOKEN")

    if not email or not token:
        print("Error: ATLASSIAN_USER_EMAIL and ATLASSIAN_API_TOKEN must be set")
        print("Create a .env file and source it, or export the variables directly.")
        return 1

    # Read notes
    if args.notes_stdin:
        notes_content = sys.stdin.read()
    elif args.notes_file:
        with open(args.notes_file) as f:
            notes_content = f.read()
    else:
        print("Error: Must specify --notes-file or --notes-stdin")
        return 1

    # Parse notes
    _, pr_notes = parse_notes_markdown(notes_content)

    # Fetch release info
    release = fetch_release_info(args.version)

    jira = JiraClient(email, token)

    # Check if version already exists
    if jira.version_exists(release.version_name):
        print(f"Error: Jira version '{release.version_name}' already exists")
        return 1

    # Create Jira version
    print(f"Creating Jira release '{release.version_name}'...")
    version_result = jira.create_version(
        name=release.version_name,
        description=f"GitHub Release: {release.github_url}",
        release_date=release.published_at.isoformat(),
    )
    version_id = version_result["id"]
    print(f"  Created with ID: {version_id}")

    # Update Fix Version on linked Jira issues
    for issue_key in release.jira_issues:
        print(f"Updating Fix Version on {issue_key}...")
        jira.update_issue_fix_version(issue_key, version_id)

    # Create QA Story
    print("Creating QA testing story...")
    qa_description = build_adf_from_notes(release, pr_notes)
    qa_result = jira.create_issue(
        summary=f"QA Testing Notes for {release.version_name}",
        description=qa_description,
        fix_version_id=version_id,
    )
    qa_key = qa_result["key"]

    # Summary
    print()
    print("Done!")
    print(f"  Jira Release: {JIRA_BASE_URL}/projects/CLI/versions/{version_id}")
    print(f"  QA Story:     {JIRA_BASE_URL}/browse/{qa_key}")
    if release.jira_issues:
        print(f"  Updated Fix Version on: {', '.join(release.jira_issues)}")

    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Create a Jira release for ana-cli with QA testing notes.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Step 1: Generate notes for review
  %(prog)s v0.0.9 --generate-notes > qa_notes.md

  # Step 2: Review and edit qa_notes.md, then create the release
  %(prog)s v0.0.9 --notes-file qa_notes.md

  # Or pipe directly:
  cat qa_notes.md | %(prog)s v0.0.9 --notes-stdin
""",
    )
    parser.add_argument("version", help="Release version (e.g., v0.0.9)")
    parser.add_argument(
        "--generate-notes",
        action="store_true",
        help="Generate QA notes markdown for review (stdout)",
    )
    parser.add_argument(
        "--notes-file",
        metavar="FILE",
        help="Path to reviewed notes markdown file",
    )
    parser.add_argument(
        "--notes-stdin",
        action="store_true",
        help="Read reviewed notes from stdin",
    )

    args = parser.parse_args()

    if args.generate_notes:
        return cmd_generate_notes(args)
    elif args.notes_file or args.notes_stdin:
        return cmd_create_release(args)
    else:
        parser.print_help()
        print("\nError: Must specify --generate-notes, --notes-file, or --notes-stdin")
        return 1


if __name__ == "__main__":
    sys.exit(main())
