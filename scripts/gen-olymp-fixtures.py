#!/usr/bin/env python3
# Regenerate the contest-en and contest-zh-mixed fixtures from one description:
#   scripts/gen-olymp-fixtures.py tests/fixtures/olymp-ref
# writes the reference template's Markdown and, through scripts/fixture-to-workspace.py,
# the same content as a workspace; then rebuild their reference.pdf with
# scripts/build-olymp-ref.sh.
"""Generate held-out olymp fixtures: the same content as reference Markdown and as a workspace."""
import subprocess
import sys
from pathlib import Path

root = Path(sys.argv[1])

# Each block (the Typst forms are left from Phase 0 and unused): ("p", md, typ) paragraph; ("h", key) section; ("dm", md, typ) display math;
# ("ul", [(md, typ)...]); ("samples", [(in, out)...]).
SECTION_MD = {
    "chinese": {"input": "输入格式", "output": "输出格式", "note": "提示", "explanation": "样例解释",
                "scoring": "评分", "constraints": "约束", "interaction": "交互"},
    "english": {"input": "Input", "output": "Output", "note": "Note", "explanation": "Explanation",
                 "scoring": "Scoring", "constraints": "Constraints", "interaction": "Interaction"},
}


def problem_md(p, language):
    meta = [f'title: "{p["title"]}"']
    for k in ("input-file", "output-file", "time-limit", "memory-limit"):
        meta.append(f"{k}: {p[k]}")
    if not p.get("metadata", True):
        meta.append("show-metadata: false")
    out = ["---", *meta, "---", ""]
    for b in p["body"]:
        kind = b[0]
        if kind == "p":
            out += [b[1], ""]
        elif kind == "h":
            out += [f"# {SECTION_MD[language][b[1]]} {{#{b[1]}}}", ""]
        elif kind == "dm":
            out += ["$$", b[1], "$$", ""]
        elif kind == "ul":
            out += [f"- {md}" for md, _ in b[1]] + [""]
        elif kind == "samples":
            out.append("::: {.samples}")
            for i, o in b[1]:
                out += ["``` {.input}", i, "```", "", "``` {.output}", o, "```", ""]
            out += [":::", ""]
    return "\n".join(out)


def write(name, contest, problems):
    d = root / name
    (d / "problems").mkdir(parents=True, exist_ok=True)
    labels = [chr(ord("A") + i) for i in range(len(problems))]
    items = "\n".join(f"  - index: {l}\n    source: problems/{l.lower()}.md" for l in labels)
    (d / "contest.md").write_text(
        f"---\nname: {contest['name']}\nlocation: {contest['location']}\ndate: {contest['date']}\n"
        f"author: {contest['author']}\nlanguage: {contest['language']}\nshow-metadata: true\nproblems:\n{items}\n---\n"
    )
    for l, p in zip(labels, problems):
        (d / "problems" / f"{l.lower()}.md").write_text(problem_md(p, contest["language"]))
    subprocess.run([sys.executable, Path(__file__).with_name("fixture-to-workspace.py"), d], check=True)


EN_STD = {"input-file": "standard input", "output-file": "standard output"}
write(
    "contest-en",
    dict(name="XCPC Workstation Open Cup", location="Shanghai", date="November 20, 2026", author="Problem Setters",
         language="english"),
    [
        dict(title="Yogurt Grouping", **EN_STD, **{"time-limit": "2 seconds", "memory-limit": "256 megabytes"}, body=[
            ("p", "Polycarp keeps $n$ cups of yogurt in a row; the $i$-th cup has flavour $a_i$. He wants to split the row into the minimum number of contiguous groups so that no group contains two cups of the same flavour, because mixing identical flavours is boring.",
                  "Polycarp keeps $n$ cups of yogurt in a row; the $i$-th cup has flavour $a_i$. He wants to split the row into the minimum number of contiguous groups so that no group contains two cups of the same flavour, because mixing identical flavours is boring."),
            ("p", "Formally, you have to find the smallest $k$ such that the sequence can be cut into $k$ segments, and for every segment the following quantity is equal to its length:",
                  "Formally, you have to find the smallest $k$ such that the sequence can be cut into $k$ segments, and for every segment the following quantity is equal to its length:"),
            ("dm", r"\left|\{a_l, a_{l+1}, \ldots, a_r\}\right| = r - l + 1.", r"lr(|\\{a_l, a_(l+1), dots, a_r\\}|) = r - l + 1."),
            ("p", "Note that a single cup always forms a valid group. Some useful facts:", "Note that a single cup always forms a valid group. Some useful facts:"),
            ("ul", [("a greedy cut is optimal;", "a greedy cut is optimal;"),
                    ("each flavour is remembered with its last position;", "each flavour is remembered with its last position;"),
                    ("the answer never exceeds $n$.", "the answer never exceeds $n$.")]),
            ("h", "input"),
            ("p", r"The first line contains an integer $n$ ($1 \le n \le 2 \cdot 10^5$). The second line contains $n$ integers $a_1, a_2, \ldots, a_n$ ($1 \le a_i \le 10^9$).",
                  r"The first line contains an integer $n$ ($1 <= n <= 2 dot 10^5$). The second line contains $n$ integers $a_1, a_2, dots, a_n$ ($1 <= a_i <= 10^9$)."),
            ("h", "output"),
            ("p", "Print one integer: the minimum number of groups.", "Print one integer: the minimum number of groups."),
            ("samples", [("5\n1 2 1 3 2", "2"), ("3\n7 7 7", "3")]),
            ("h", "explanation"),
            ("p", "In the first example the groups are $[1, 2]$ and $[1, 3, 2]$.", "In the first example the groups are $\\[1, 2\\]$ and $\\[1, 3, 2\\]$."),
        ]),
        dict(title="Quiet Problem", **EN_STD, **{"time-limit": "1 second", "memory-limit": "512 megabytes"}, metadata=False, body=[
            ("p", "Read a single word and print it back in reverse order. This problem hides its limits table.",
                  "Read a single word and print it back in reverse order. This problem hides its limits table."),
            ("h", "input"),
            ("p", "One line with a word of at most $100$ lowercase letters.", "One line with a word of at most $100$ lowercase letters."),
            ("h", "output"),
            ("p", "The reversed word.", "The reversed word."),
            ("samples", [("typography", "yhpargopyt")]),
            ("h", "note"),
            ("p", "Long lines in samples are kept as they are.", "Long lines in samples are kept as they are."),
        ]),
    ],
)

LONG_ZH = [
    "在一个遥远的王国里，有 $n$ 座城市和 $m$ 条双向道路。国王希望修建若干座仓库，使得每座城市到最近仓库的距离都不超过 $d$，同时仓库的数量尽可能少。由于预算有限，每条道路只能被维修一次，维修的代价与道路长度成正比。",
    "为了简化问题，我们假设所有道路的长度都是正整数，并且任意两座城市之间至多只有一条道路直接相连。图可能不连通，此时每个连通块都需要独立地满足上述条件。",
    "王国的工程师们已经测量了所有道路的长度，并把结果写在了一张很长的清单上。清单上的每一行都描述了一条道路的两个端点以及它的长度。工程师们还注意到，有一些城市地处偏远，只与一条道路相连。",
    "请你帮助国王计算最少需要修建多少座仓库。如果无论如何都无法满足条件，请输出 $-1$。注意，仓库只能修建在城市中，而不能修建在道路的中间。",
]
write(
    "contest-zh-mixed",
    dict(name="XCPC 工作站留出测试", location="北京", date="2026 年 12 月 1 日", author="命题组", language="chinese"),
    [
        dict(title="Typography 仓库", **{"input-file": "标准输入", "output-file": "标准输出", "time-limit": "3 秒", "memory-limit": "1024 MB"}, body=[
            *[("p", t, t.replace("$-1$", "$-1$")) for t in LONG_ZH * 3],
            ("h", "input"),
            ("p", r"第一行三个整数 $n,m,d$（$1 \le n,m \le 10^5$，$1 \le d \le 10^9$）。",
                  r"第一行三个整数 $n,m,d$（$1 <= n,m <= 10^5$，$1 <= d <= 10^9$）。"),
            ("h", "output"),
            ("p", "输出一个整数表示答案。", "输出一个整数表示答案。"),
            ("samples", [("3 2 1\n1 2 1\n2 3 1", "1")]),
        ]),
        dict(title="评分与约束", **{"input-file": "标准输入", "output-file": "标准输出", "time-limit": "1 秒", "memory-limit": "256 MB"}, metadata=False, body=[
            ("p", "本题没有样例，用于检验没有限制表时题头之后的间距。", "本题没有样例，用于检验没有限制表时题头之后的间距。"),
            ("h", "constraints"),
            ("ul", [(r"$1 \le n \le 10$；", "$1 <= n <= 10$；"), (r"$1 \le a_i \le 100$。", "$1 <= a_i <= 100$。")]),
            ("h", "scoring"),
            ("p", "共 $10$ 个测试点，每个测试点 $10$ 分。", "共 $10$ 个测试点，每个测试点 $10$ 分。"),
        ]),
        dict(title="样例之后", **{"input-file": "标准输入", "output-file": "标准输出", "time-limit": "1 秒", "memory-limit": "256 MB"}, body=[
            ("p", "给定一个字符串，输出它的长度。", "给定一个字符串，输出它的长度。"),
            ("samples", [("hello", "5"), ("xcpc", "4")]),
            ("p", "样例之后直接是一段正文，没有章节标题。", "样例之后直接是一段正文，没有章节标题。"),
        ]),
    ],
)
print("ok")
