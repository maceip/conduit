#!/usr/bin/env python3
"""Point the site's links at the right place.

The site holds the documents, at its root: `architecture.html`,
`research/app-catalogue.html`, and so on.  The code is on GitHub.

A link is resolved against the document it is on.  If it lands on a
document, it points at the page the site built for it; if it lands on
a file in the repository, it points at that file there.

    python3 scripts/pages-links.py _site
"""

from __future__ import annotations

import os
import re
import sys

#: Where the repository is.
REPOSITORY = "https://github.com/maceip/conduit/blob/main"

#: The directory the documents live in, in the repository.
DOCUMENTS = "docs"

#: The directory the schemas live in, and which the site holds whole.
SCHEMAS = "spec"

#: A link in the html.
LINK = re.compile(r'href="([^"]+)"')


def the_source_directory(page: str) -> str:
    """Where the document behind a page of the site lives."""
    if page == "index.html":
        # The index is the README, which is at the root.
        return ""
    return os.path.join(DOCUMENTS, os.path.dirname(page)).replace(os.sep, "/")


def rewrite(href: str, page: str) -> str:
    """Where a link on that page should point."""
    if href.startswith(("http://", "https://", "#", "mailto:", "data:")):
        return href

    # The link, resolved against the document it is on.
    base = the_source_directory(page)
    target = os.path.normpath(os.path.join(base, href)).replace(os.sep, "/")
    if target.startswith("/"):
        target = target[1:]

    if target.startswith(f"{DOCUMENTS}/"):
        # A document: the site has the page for it, at its root.
        on_the_site = target[len(DOCUMENTS) + 1 :]
        if on_the_site.endswith(".md"):
            on_the_site = on_the_site[:-3] + ".html"
        return on_the_site
    if target.startswith(f"{SCHEMAS}/"):
        # The schemas: the site holds them whole.
        return target
    if target in (DOCUMENTS, SCHEMAS):
        return target

    # A file in the repository.
    return f"{REPOSITORY}/{target}"


def main(directory: str) -> int:
    """Rewrite the links of every html file under the directory."""
    changed = 0
    for root, _, files in os.walk(directory):
        for name in files:
            if not name.endswith(".html"):
                continue
            path = os.path.join(root, name)
            page = os.path.relpath(path, directory).replace(os.sep, "/")
            with open(path, encoding="utf-8") as handle:
                text = handle.read()
            fixed = LINK.sub(
                lambda match: f'href="{rewrite(match.group(1), page)}"', text
            )
            if fixed != text:
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(fixed)
                changed += 1
    print(f"the links of {changed} page(s) were pointed at the right place")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
