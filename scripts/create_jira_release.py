#!/usr/bin/env python3
"""
Create a Jira release for ana-cli and generate QA testing notes.

Usage:
    python scripts/create_jira_release.py v0.0.8

Required environment variables:
    ATLASSIAN_USER_EMAIL - Your Atlassian account email
    ATLASSIAN_API_TOKEN  - API token from https://id.atlassian.com/manage-profile/security/api-tokens

You can set these in a .env file and source it before running.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
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
        "release", "view", version, "--repo", GITHUB_REPO, "--json", "tagName,body,url"
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


def build_pr_list(pr_numbers: list[int]) -> list[PullRequest]:
    """Fetch details for each PR and build PullRequest objects."""
    prs = []
    for num in pr_numbers:
        print(f"  Fetching PR #{num}...")
        try:
            details = get_pr_details(num)
            author = details.get("author", {}).get("login", "unknown")
            is_bot = "renovate" in author.lower() or "bot" in author.lower()

            prs.append(
                PullRequest(
                    number=num,
                    title=details.get("title", f"PR #{num}"),
                    author=author,
                    jira_link=extract_jira_link(details.get("body", "")),
                    is_bot=is_bot,
                )
            )
        except subprocess.CalledProcessError:
            print(f"    Warning: Could not fetch PR #{num}")

    return prs


def build_adf_document(github_url: str, prs: list[PullRequest]) -> dict:
    """Build Atlassian Document Format for QA testing notes."""
    content: list[dict[str, Any]] = []

    # Header
    content.append(heading("Release Info", level=2))
    content.append(
        paragraph(
            text("GitHub Release: "),
            link("Release Notes", github_url),
        )
    )

    # Group PRs by category (excluding bots)
    non_bot_prs = [pr for pr in prs if not pr.is_bot]
    features = [pr for pr in non_bot_prs if pr.category == "feature"]
    fixes = [pr for pr in non_bot_prs if pr.category == "fix"]
    maintenance = [pr for pr in non_bot_prs if pr.category == "maintenance"]
    other = [pr for pr in non_bot_prs if pr.category == "other"]

    def pr_list_item(pr: PullRequest) -> dict:
        return list_item(
            paragraph(
                link(f"PR #{pr.number}", pr.url, bold=True),
                text(f" - {pr.title}"),
            )
        )

    if features:
        content.append(heading("Features", level=2))
        content.append(bullet_list([pr_list_item(pr) for pr in features]))

    if fixes:
        content.append(heading("Bug Fixes", level=2))
        content.append(bullet_list([pr_list_item(pr) for pr in fixes]))

    if maintenance:
        content.append(heading("Maintenance", level=2))
        content.append(bullet_list([pr_list_item(pr) for pr in maintenance]))

    if other:
        content.append(heading("Other Changes", level=2))
        content.append(bullet_list([pr_list_item(pr) for pr in other]))

    return {"type": "doc", "version": 1, "content": content}


# ADF helper functions
def text(value: str, bold: bool = False, code: bool = False) -> dict:
    node = {"type": "text", "text": value}
    marks = []
    if bold:
        marks.append({"type": "strong"})
    if code:
        marks.append({"type": "code"})
    if marks:
        node["marks"] = marks
    return node


def link(label: str, url: str, bold: bool = False) -> dict:
    marks = [{"type": "link", "attrs": {"href": url}}]
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


def main():
    if len(sys.argv) < 2:
        print("Usage: python scripts/create_jira_release.py <version>")
        print("Example: python scripts/create_jira_release.py v0.0.8")
        sys.exit(1)

    version = sys.argv[1]
    version_name = f"ana-cli {version}"

    # Check environment
    email = os.environ.get("ATLASSIAN_USER_EMAIL")
    token = os.environ.get("ATLASSIAN_API_TOKEN")

    if not email or not token:
        print("Error: ATLASSIAN_USER_EMAIL and ATLASSIAN_API_TOKEN must be set")
        print("Create a .env file and source it, or export the variables directly.")
        sys.exit(1)

    jira = JiraClient(email, token)

    # Check if version already exists
    if jira.version_exists(version_name):
        print(f"Error: Jira version '{version_name}' already exists")
        sys.exit(1)

    # Get GitHub release
    print(f"Fetching GitHub release {version}...")
    release = get_github_release(version)
    github_url = release["url"]

    # Extract and process PRs
    print("Extracting PRs from release notes...")
    pr_numbers = extract_pr_numbers(release["body"])
    prs = build_pr_list(pr_numbers)

    # Create Jira version
    print(f"Creating Jira release '{version_name}'...")
    from datetime import date

    version_result = jira.create_version(
        name=version_name,
        description=f"GitHub Release: {github_url}",
        release_date=date.today().isoformat(),
    )
    version_id = version_result["id"]
    print(f"  Created with ID: {version_id}")

    # Update Fix Version on linked Jira issues
    jira_issues = [pr.jira_link for pr in prs if pr.jira_link]
    for issue_key in jira_issues:
        print(f"Updating Fix Version on {issue_key}...")
        jira.update_issue_fix_version(issue_key, version_id)

    # Create QA Story
    print("Creating QA testing story...")
    qa_description = build_adf_document(github_url, prs)
    qa_result = jira.create_issue(
        summary=f"QA Testing Notes for {version_name}",
        description=qa_description,
        fix_version_id=version_id,
    )
    qa_key = qa_result["key"]

    # Summary
    print()
    print("Done!")
    print(f"  Jira Release: {JIRA_BASE_URL}/projects/CLI/versions/{version_id}")
    print(f"  QA Story:     {JIRA_BASE_URL}/browse/{qa_key}")
    if jira_issues:
        print(f"  Updated Fix Version on: {', '.join(jira_issues)}")


if __name__ == "__main__":
    main()
