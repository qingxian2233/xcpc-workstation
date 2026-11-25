#!/usr/bin/env node
// A fake OpenAI-compatible model service for exercising the problem setter end to end:
// `node scripts/mock-llm.mjs [port]`, then set the API base URL to http://127.0.0.1:<port>/v1.
//
// It writes a range-sum problem. The first draft's std has an off-by-one bug and one sample
// answer is wrong on purpose, so stress testing, sample correction and the repair round
// all get exercised; the repair request gets the fixed program back.
import { createServer } from "node:http";

const port = Number(process.argv[2] ?? 4600);

const ideas = {
  ideas: [
    {
      title: "灯塔巡检",
      summary: "给定长为 n 的序列和 q 次询问，每次询问区间 [l, r] 的元素和。",
      solution: "前缀和预处理，每次询问 O(1) 作差。",
      complexity: "O(n + q)",
      killed: [{ approach: "每次询问直接循环求和", how: "n = q = 2×10^5，总复杂度 O(nq) 超时" }],
      rating: 900,
    },
    { title: "补给线", summary: "树上路径和查询。", solution: "树上前缀和 + LCA。", complexity: "O((n+q) log n)", killed: [], rating: 1600 },
    { title: "潮汐", summary: "带修改的区间和。", solution: "树状数组。", complexity: "O((n+q) log n)", killed: [], rating: 1300 },
  ],
};

const std = (bug) => `#include <bits/stdc++.h>
using namespace std;
int main(){int n,q;scanf("%d %d",&n,&q);vector<long long>p(n+1,0);
for(int i=1;i<=n;i++){long long x;scanf("%lld",&x);p[i]=p[i-1]+x;}
while(q--){int l,r;scanf("%d %d",&l,&r);printf("%lld\\n",p[r]-p[l${bug ? "" : "-1"}]);}}
`;
const brute = `#include <bits/stdc++.h>
using namespace std;
int main(){int n,q;cin>>n>>q;vector<long long>a(n+1);for(int i=1;i<=n;i++)cin>>a[i];
while(q--){int l,r;cin>>l>>r;long long s=0;for(int i=l;i<=r;i++)s+=a[i];cout<<s<<"\\n";}}
`;
const generator = `#include <bits/stdc++.h>
using namespace std;
int main(int argc,char**argv){bool small=string(argv[1])=="small";mt19937_64 rng(atoll(argv[2]));
int n=small?rng()%8+1:200000,q=small?rng()%8+1:200000;long long V=small?10:1000000000;
printf("%d %d\\n",n,q);for(int i=0;i<n;i++)printf("%lld ",(long long)(rng()%V)+1);puts("");
for(int i=0;i<q;i++){int l=small?rng()%n+1:rng()%10+1,r=small?rng()%n+1:n-(int)(rng()%10);if(l>r)swap(l,r);printf("%d %d\\n",l,r);}}
`;
const wrong = `#include <bits/stdc++.h>
using namespace std;
int main(){int n,q;scanf("%d %d",&n,&q);vector<long long>a(n+1);for(int i=1;i<=n;i++)scanf("%lld",&a[i]);
while(q--){int l,r;scanf("%d %d",&l,&r);long long s=0;for(int i=l;i<=r;i++)s+=a[i];printf("%lld\\n",s);}}
`;

const draft = (bug) => ({
  slug: "lighthouse-patrol",
  title: "灯塔巡检",
  rating: 900,
  tags: ["prefix sums", "implementation"],
  limits: { timeMs: 1000, memoryMb: 256 },
  background: "海岸线上有一排灯塔，巡检员每天要统计一段灯塔的总能耗。",
  legend: "给定长度为 $n$ 的序列 $a_1, a_2, \\ldots, a_n$，以及 $q$ 次询问。每次询问给出 $l, r$，求 $\\sum_{i=l}^{r} a_i$。",
  input: "第一行两个整数 $n, q$。\n\n第二行 $n$ 个整数 $a_1, \\ldots, a_n$。\n\n接下来 $q$ 行，每行两个整数 $l, r$。",
  output: "对每次询问输出一行一个整数，表示区间和。",
  constraints: "$1 \\le n, q \\le 2 \\times 10^5$，$1 \\le a_i \\le 10^9$，$1 \\le l \\le r \\le n$。",
  note: "",
  samples: [
    { input: "5 3\n1 2 3 4 5\n1 3\n2 5\n4 4\n", output: "6\n14\n4\n", explanation: "$1+2+3=6$，$2+3+4+5=14$，第三个区间只有 $a_4=4$。" },
    { input: "3 2\n1000000000 1000000000 1000000000\n1 3\n2 2\n", output: "3000000000\n1000000001\n", explanation: "答案会超过 32 位整数的范围，需要用 64 位整数。" },
  ],
  editorial: "预处理前缀和 $s_i = \\sum_{j \\le i} a_j$，答案为 $s_r - s_{l-1}$。逐个累加的做法是 $O(nq)$ 的，在 $n = q = 2 \\times 10^5$ 时会超时。",
  std: std(bug),
  brute,
  generator,
  wrong: [{ name: "wrong-loop", approach: "每次询问直接循环求和", code: wrong }],
});

// Answers in the wire format the request came in: Chat Completions, Responses or Messages.
const replyAs = (format) => (res, content) => {
  res.writeHead(200, { "content-type": "application/json" });
  if (format === "responses")
    return res.end(JSON.stringify({ output: [{ type: "reasoning", summary: [] }, { type: "message", role: "assistant", content: [{ type: "output_text", text: content }] }] }));
  if (format === "messages")
    return res.end(JSON.stringify({ type: "message", role: "assistant", content: [{ type: "thinking", thinking: "" }, { type: "text", text: content }], stop_reason: "end_turn" }));
  res.end(JSON.stringify({ choices: [{ message: { role: "assistant", content } }] }));
};

const fail = (res, status, message) => {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: { message } }));
};

createServer((req, res) => {
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    if (req.url.endsWith("/models")) {
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify({ data: [{ id: "mock-setter" }, { id: "mock-setter-pro" }] }));
    }
    const req_ = JSON.parse(body || "{}");
    const format = req.url.endsWith("/responses") ? "responses" : req.url.endsWith("/messages") ? "messages" : "chat";
    // Check each format's required shape, as the real services would.
    if (format === "messages") {
      if (!req.headers["x-api-key"] || !req.headers["anthropic-version"]) return fail(res, 401, "missing x-api-key or anthropic-version");
      if (!req_.max_tokens) return fail(res, 400, "max_tokens: field required");
      if ((req_.messages ?? []).some((m) => m.role === "system")) return fail(res, 400, "system must be top-level");
    } else if (!(req.headers.authorization ?? "").startsWith("Bearer ")) return fail(res, 401, "missing bearer token");
    if (format === "responses" && !Array.isArray(req_.input)) return fail(res, 400, "input: field required");
    console.log(`${format} ${req.url} effort=${JSON.stringify(req_.reasoning ?? req_.output_config ?? req_.reasoning_effort ?? null)}`);
    const reply = replyAs(format);
    const msgs = req_.messages ?? req_.input ?? [];
    const last = msgs.at(-1)?.content ?? "";
    // Pretend to think a little so the progress UI has something to show.
    setTimeout(() => {
      if (last.includes("构思 3 个")) return reply(res, JSON.stringify(ideas));
      if (last.includes("自动验证发现了以下问题")) return reply(res, "```json\n" + JSON.stringify(draft(false)) + "\n```");
      if (last.includes("修改要求")) {
        const d = draft(false);
        d.title = "灯塔巡检（修订）";
        return reply(res, JSON.stringify(d));
      }
      return reply(res, JSON.stringify(draft(true)));
    }, 800);
  });
}).listen(port, "127.0.0.1", () => console.log(`mock model service on http://127.0.0.1:${port}/v1`));
