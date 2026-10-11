#!/usr/bin/env python3
"""Generate data/catalog.json, the offline problem catalog embedded in the grind binary.

Run by hand with `python3 -I scripts/gen_catalog.py`. Needs network access to leetcode.com.

scripts/lists.json holds two ordered slug lists, vendored once:
  grind75      Grind 75 order from techinterviewhandbook.org/grind75, the selection its page
               computes from the public priority table with its default 8 weeks x 8 hours budget.
  neetcode150  entries with "neetcode150": true from
               https://raw.githubusercontent.com/neetcode-gh/leetcode/main/.problemSiteData.json,
               in file order, slug taken from "link".

Problems are ordered Grind 75 first, then NeetCode-only problems in NeetCode order.
Statements come from LeetCode's public GraphQL endpoint and are converted to the markdown-lite
grammar that src/tui/markdown.rs renders. Premium problems carry no statement or starters.
"""

import html.parser
import json
import os
import re
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
LISTS_PATH = os.path.join(HERE, "lists.json")
OUT_PATH = os.path.join(HERE, "..", "data", "catalog.json")
GRAPHQL_URL = "https://leetcode.com/graphql"
QUERY = (
    "query q($s: String!){question(titleSlug:$s){questionFrontendId title titleSlug "
    "difficulty isPaidOnly content codeSnippets{langSlug code} exampleTestcases}}"
)
STARTER_LANGS = {
    "python3": "python3",
    "rust": "rust",
    "javascript": "javascript",
    "typescript": "typescript",
    "java": "java",
    "cpp": "cpp",
    "golang": "go",
    "c": "c",
}
DIFFICULTIES = ("Easy", "Medium", "Hard")
ENTITY_RE = re.compile(r"&(?:#[0-9]+|#[xX][0-9a-fA-F]+|[A-Za-z][A-Za-z0-9]*);")
BLOCK_TAGS = {"p", "div", "ul", "ol", "h1", "h2", "h3", "h4", "h5", "h6", "table", "tbody", "thead", "tr", "td", "th", "hr", "blockquote"}
INLINE_TAGS = {"a", "b", "br", "code", "em", "font", "i", "img", "li", "pre", "span", "strong", "sub", "sup", "u"}
KNOWN_TAGS = BLOCK_TAGS | INLINE_TAGS
TAG_RE = re.compile(r"</?(?:%s)(?:\s[^<>]*)?/?>" % "|".join(sorted(KNOWN_TAGS)))


def _tidy(text):
    text = re.sub(r"\*\*\s*\*\*", " ", text)
    lines = [re.sub(r" {2,}", " ", line).strip() for line in text.split("\n")]
    text = "\n".join(line for line in lines if line)
    # Move whitespace that landed inside a bold span out to its edges.
    return re.sub(r"\*\*( *)([^*\n]*?)( *)\*\*", lambda m: m.group(1) + "**" + m.group(2) + "**" + m.group(3), text)


def _pre_block(raw):
    lines = [line.rstrip() for line in raw.replace("\r", "").split("\n")]
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    return "\n".join("    " + line for line in lines)


class _Markdown(html.parser.HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.blocks = []
        self.cur = []
        self.cur_kind = "p"
        self.in_pre = False
        self.pre = []
        self.unknown_tags = set()

    def _emit(self, text):
        (self.pre if self.in_pre else self.cur).append(text)

    def _flush(self):
        text = _tidy("".join(self.cur))
        if text and text != "-":
            self.blocks.append((self.cur_kind, text))
        self.cur = []
        self.cur_kind = "p"

    def handle_starttag(self, tag, attrs):
        if tag not in KNOWN_TAGS:
            self.unknown_tags.add(tag)
        if tag == "img":
            self._emit("[image]")
        elif self.in_pre:
            return
        elif tag == "pre":
            self._flush()
            self.in_pre = True
            self.pre = []
        elif tag in BLOCK_TAGS:
            self._flush()
        elif tag == "li":
            self._flush()
            self.cur_kind = "li"
            self.cur.append("- ")
        elif tag in ("strong", "b"):
            self.cur.append("**")
        elif tag == "code":
            self.cur.append("`")
        elif tag == "sup":
            self.cur.append("^")
        elif tag == "sub":
            self.cur.append("_")
        elif tag == "br":
            self.cur.append("\n")

    def handle_endtag(self, tag):
        if tag == "pre" and self.in_pre:
            self.in_pre = False
            self.blocks.append(("pre", _pre_block("".join(self.pre))))
        elif self.in_pre:
            return
        elif tag in ("strong", "b"):
            self.cur.append("**")
        elif tag == "code":
            self.cur.append("`")
        elif tag == "li" or tag in BLOCK_TAGS:
            self._flush()

    def handle_data(self, data):
        data = data.replace("\xa0", " ")
        if self.in_pre:
            self.pre.append(data)
        else:
            self.cur.append(re.sub(r"\s+", " ", data))


def to_markdown_lite(source):
    parser = _Markdown()
    parser.feed(source)
    parser.close()
    parser._flush()
    if parser.unknown_tags:
        raise SystemExit(f"unhandled HTML tags: {sorted(parser.unknown_tags)}")
    out = ""
    prev = None
    for kind, text in parser.blocks:
        if not text:
            continue
        if out:
            out += "\n" if (prev == "li" and kind == "li") else "\n\n"
        out += text
        prev = kind
    return out


def fetch(slug):
    payload = json.dumps({"query": QUERY, "variables": {"s": slug}}).encode("utf-8")
    request = urllib.request.Request(
        GRAPHQL_URL,
        data=payload,
        headers={
            "Content-Type": "application/json",
            "Referer": "https://leetcode.com",
            "User-Agent": "Mozilla/5.0",
        },
    )
    last_error = None
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                question = json.load(response)["data"]["question"]
            if question is None or question["titleSlug"] != slug:
                raise SystemExit(f"{slug}: LeetCode returned no matching question")
            return question
        except (OSError, ValueError, KeyError, TypeError) as error:
            last_error = error
            time.sleep(1 + attempt)
    raise SystemExit(f"{slug}: fetch failed after retries: {last_error}")


def build_problem(slug, question):
    premium = question["isPaidOnly"]
    problem = {
        "id": int(question["questionFrontendId"]),
        "slug": slug,
        "title": question["title"],
        "difficulty": question["difficulty"],
        "examples": question["exampleTestcases"],
    }
    if problem["difficulty"] not in DIFFICULTIES:
        raise SystemExit(f"{slug}: unexpected difficulty {problem['difficulty']!r}")
    if premium:
        problem["body"] = {"kind": "premium"}
        return problem
    starters = {
        STARTER_LANGS[snippet["langSlug"]]: snippet["code"]
        for snippet in question["codeSnippets"] or []
        if snippet["langSlug"] in STARTER_LANGS
    }
    if "python3" not in starters:
        raise SystemExit(f"{slug}: no python3 starter")
    description = to_markdown_lite(question["content"] or "")
    if not description:
        raise SystemExit(f"{slug}: empty statement")
    for field, text in (("description", description), ("examples", problem["examples"])):
        if TAG_RE.search(text) or ENTITY_RE.search(text):
            raise SystemExit(f"{slug}: markup survived in {field}")
    problem["body"] = {"kind": "free", "description": description, "starters": starters}
    return problem


def main():
    with open(LISTS_PATH, encoding="utf-8") as handle:
        lists = json.load(handle)
    grind75 = lists["grind75"]
    neetcode150 = lists["neetcode150"]
    problems = []
    for slug in dict.fromkeys(grind75 + neetcode150):
        problems.append(build_problem(slug, fetch(slug)))
        time.sleep(0.1)
    ids = [problem["id"] for problem in problems]
    if len(set(ids)) != len(ids):
        raise SystemExit("duplicate LeetCode frontend ids")

    catalog = {"lists": {"grind75": grind75, "neetcode150": neetcode150}, "problems": problems}
    text = json.dumps(catalog, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    with open(OUT_PATH, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(text)

    premium = sum(1 for problem in problems if problem["body"]["kind"] == "premium")
    print(f"{len(problems)} problems, {len(grind75)} grind75, {len(neetcode150)} neetcode150, {premium} premium")


if __name__ == "__main__":
    main()
