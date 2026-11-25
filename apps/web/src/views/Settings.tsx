import { useEffect, useState } from "react";
import { api, type ApiFormat, type LlmSettings, type SetterEnv } from "../api";
import { Icon } from "../components/Icon";

const EFFORTS: [LlmSettings["reasoningEffort"], string][] = [
  ["auto", "默认"],
  ["low", "低"],
  ["medium", "中"],
  ["high", "高"],
];

// Each wire format with the hint that helps pick it and sensible placeholders.
const FORMATS: { id: ApiFormat; label: string; path: string; hint: string; url: string; model: string }[] = [
  { id: "chat", label: "Chat Completions", path: "/chat/completions", hint: "大多数服务都支持：DeepSeek、通义、Kimi、智谱、Ollama、各类中转", url: "https://api.deepseek.com/v1", model: "deepseek-reasoner" },
  { id: "responses", label: "Responses", path: "/responses", hint: "OpenAI 的新接口，推理模型推荐用它", url: "https://api.openai.com/v1", model: "gpt-5" },
  { id: "messages", label: "Messages", path: "/messages", hint: "Anthropic Claude 的接口，也适用于兼容它的服务", url: "https://api.anthropic.com/v1", model: "claude-opus-5-5" },
];

const isMac = navigator.platform.includes("Mac") || navigator.userAgent.includes("Mac OS");

export function Settings() {
  const [s, setS] = useState<LlmSettings>();
  const [apiKey, setApiKey] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [busy, setBusy] = useState<"" | "save" | "models">("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string }>();
  const [env, setEnv] = useState<SetterEnv>();

  useEffect(() => {
    api.llmSettings().then(setS, () => setS({ apiFormat: "chat", baseUrl: "", model: "", reasoningEffort: "auto", apiKeyConfigured: false }));
    api.setterEnv().then(setEnv, () => {});
  }, []);
  if (!s) return null;
  const format = FORMATS.find((f) => f.id === s.apiFormat) ?? FORMATS[0];

  const set = (p: Partial<LlmSettings>) => (setS({ ...s, ...p }), setMsg(undefined));

  const save = async (quiet = false) => {
    setBusy("save");
    setMsg(undefined);
    try {
      const next = await api.saveLlmSettings({ apiFormat: s.apiFormat, baseUrl: s.baseUrl, model: s.model, reasoningEffort: s.reasoningEffort, apiKey: apiKey || undefined });
      setS(next);
      setApiKey("");
      if (!quiet) setMsg({ ok: true, text: "已保存" });
      api.setterEnv().then(setEnv, () => {});
      return true;
    } catch (e) {
      setMsg({ ok: false, text: (e as Error).message });
      return false;
    } finally {
      setBusy("");
    }
  };

  const fetchModels = async () => {
    if (!(await save(true))) return;
    setBusy("models");
    try {
      const { models } = await api.llmModels();
      setModels(models);
      setMsg({ ok: true, text: `连接正常，共 ${models.length} 个模型` });
    } catch (e) {
      setMsg({ ok: false, text: (e as Error).message });
    } finally {
      setBusy("");
    }
  };

  return (
    <div className="settings">
      <section className="settings-card">
        <header>
          <h2>模型服务</h2>
          <p>出题用的大模型。先选服务使用的接口格式，再填地址和密钥。</p>
        </header>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            save();
          }}
        >
          <label className="row-field">
            <span>接口格式</span>
            <div className="format-field">
              <span className="select-field">
                <select value={s.apiFormat} onChange={(e) => (set({ apiFormat: e.target.value as ApiFormat }), setModels([]))}>
                  {FORMATS.map((f) => (
                    <option key={f.id} value={f.id}>
                      {f.label}（{f.path}）
                    </option>
                  ))}
                </select>
              </span>
              <span className="format-hint">{format.hint}</span>
            </div>
          </label>
          <label className="row-field">
            <span>接口地址</span>
            <input value={s.baseUrl} onChange={(e) => set({ baseUrl: e.target.value })} placeholder={format.url} spellCheck={false} />
          </label>
          <label className="row-field">
            <span>API 密钥</span>
            <input
              type="password"
              value={apiKey}
              onChange={(e) => (setApiKey(e.target.value), setMsg(undefined))}
              placeholder={s.apiKeyConfigured ? "已保存（留空不修改）" : "sk-…"}
              autoComplete="off"
            />
          </label>
          <label className="row-field">
            <span>模型</span>
            <div className="inline">
              <input list="model-list" value={s.model} onChange={(e) => set({ model: e.target.value })} placeholder={format.model} spellCheck={false} />
              <datalist id="model-list">
                {models.map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
              <button type="button" className="btn outline" disabled={!s.baseUrl || !!busy} onClick={fetchModels}>
                {busy === "models" ? "连接中…" : "获取列表"}
              </button>
            </div>
          </label>
          <div className="row-field">
            <span>推理强度</span>
            <div className="segmented" role="radiogroup">
              {EFFORTS.map(([v, label]) => (
                <button type="button" key={v} role="radio" aria-checked={s.reasoningEffort === v} onClick={() => set({ reasoningEffort: v })}>
                  {label}
                </button>
              ))}
            </div>
          </div>
          <div className="settings-foot">
            <span className={msg?.ok === false ? "error-text" : "muted"} aria-live="polite">
              {msg?.text}
            </span>
            <span className="spacer" />
            <button className="btn primary" disabled={!!busy || !s.baseUrl}>
              保存
            </button>
          </div>
        </form>
        <p className="settings-note">密钥只保存在本机{isMac ? "的「钥匙串」" : ""}里。出题时会把你的需求和题库里几道同难度题的标题发给模型服务。</p>
      </section>

      <section className="settings-card">
        <header>
          <h2>本机编译器</h2>
          <p>用来自动验证生成的题：编译标程与暴力、对拍、测速、确认错误做法会被卡掉。</p>
        </header>
        {env ? (
          env.compiler.ok ? (
            <div className="compiler ok">
              <span className="sdot ok">
                <Icon name="check" size={12} />
              </span>
              <span className="mono">{env.compiler.version}</span>
            </div>
          ) : (
            <div className="compiler missing">
              <div className="compiler-status">
                <span className="sdot fail">
                  <Icon name="close" size={12} />
                </span>
                <span>还没有找到 C++ 编译器</span>
              </div>
              {isMac ? (
                <ol className="howto">
                  <li>
                    打开「终端」（启动台里搜索“终端”），粘贴下面这行并回车：
                    <CopyLine text="xcode-select --install" />
                  </li>
                  <li>在弹出的窗口里点「安装」，等几分钟装完（不需要装完整的 Xcode）。</li>
                  <li>
                    装好后在终端里运行 <code>clang++ --version</code>，能看到版本号就说明成功了。
                  </li>
                  <li>
                    回到这里点「重新检测」。
                  </li>
                </ol>
              ) : (
                <ol className="howto">
                  <li>
                    Debian / Ubuntu：<CopyLine text="sudo apt install g++" />
                  </li>
                  <li>
                    Windows：安装 <a href="https://www.msys2.org/" target="_blank" rel="noreferrer">MSYS2</a> 后运行 <code>pacman -S mingw-w64-ucrt-x86_64-gcc</code>，并把它的 bin 目录加入 PATH。
                  </li>
                  <li>装好后点「重新检测」。</li>
                </ol>
              )}
              <button className="btn outline" onClick={() => api.setterEnv().then(setEnv)}>
                <Icon name="refresh" /> 重新检测
              </button>
            </div>
          )
        ) : null}
      </section>
    </div>
  );
}

function CopyLine({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <div className="copy-line">
      <code>{text}</code>
      <button
        className="btn"
        onClick={() => {
          navigator.clipboard?.writeText(text);
          setDone(true);
          setTimeout(() => setDone(false), 1200);
        }}
      >
        {done ? "已复制" : "复制"}
      </button>
    </div>
  );
}
