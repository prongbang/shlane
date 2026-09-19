#!/usr/bin/env python3
"""Check that every relative link in the built site points at something.

The pages under `src/` are stubs that include Markdown written to be read on
GitHub, where a link like `docs/plan/15-roadmap.md` resolves against the
repository root. On the site it resolves against the page it landed on, and
mdBook rewrites the `.md` to `.html` on the way, so it fails silently and only
in the browser. Neither `mdbook build` nor `tests/docs_site.rs` sees it: the
first exits 0 regardless, the second reads Markdown rather than the output.

Run it against the built book:

    mdbook build docs/site && python3 docs/site/check-links.py docs/site/book

The fix for anything it reports is an absolute URL in the source Markdown —
the site for prose, GitHub for repository files — which is also what makes the
README's links work on crates.io.
"""

import os
import re
import sys

# mdBook generates the sidebar and the navigation links itself; only the page
# body comes from the Markdown this repository writes.
BODY = re.compile(r"<main>(.*?)</main>", re.S)
HREF = re.compile(r'href="([^"]+)"')
EXTERNAL = ("http://", "https://", "mailto:", "#")


def broken(root):
    for dirpath, _, filenames in os.walk(root):
        for filename in sorted(filenames):
            if not filename.endswith(".html"):
                continue
            page = os.path.join(dirpath, filename)
            with open(page, encoding="utf-8") as handle:
                body = BODY.search(handle.read())
            if not body:
                continue
            for href in HREF.findall(body.group(1)):
                if href.startswith(EXTERNAL):
                    continue
                target = href.split("#")[0]
                if not target:
                    continue
                resolved = os.path.normpath(os.path.join(dirpath, target))
                if not os.path.exists(resolved):
                    yield page, href


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "docs/site/book"
    if not os.path.isdir(root):
        sys.exit(f"{root} does not exist — run `mdbook build docs/site` first")

    found = list(broken(root))
    if not found:
        print(f"every relative link in {root} resolves")
        return
    for page, href in found:
        print(f"{page}: {href} points at nothing", file=sys.stderr)
    sys.exit(
        f"\n{len(found)} broken link(s). A link written for GitHub resolves "
        f"differently on the site; make it absolute in the source Markdown."
    )


if __name__ == "__main__":
    main()
