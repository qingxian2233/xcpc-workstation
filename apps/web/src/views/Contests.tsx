import { useState, type FormEvent } from "react";
import { api } from "../api";
import { Icon } from "../components/Icon";
import { navigate } from "../lib/router";
import { useLoad } from "../lib/hooks";

export function Contests() {
  const { data, error, reload } = useLoad(() => api.contests(), []);
  const [newContest, setNewContest] = useState(false);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [createError, setCreateError] = useState<string>();

  const create = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || saving) return;
    setSaving(true);
    setCreateError(undefined);
    try {
      const contest = await api.createContest(name.trim());
      setNewContest(false);
      setName("");
      reload();
      navigate({ name: "studio", kind: "contest", id: contest.id, editorial: false });
    } catch (err) {
      setCreateError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="contest-home">
      <div className="contest-toolbar">
        <span className="muted">{data?.length ? `${data.length} 场比赛` : ""}</span>
        <span className="spacer" />
        <button className="btn primary" onClick={() => (setCreateError(undefined), setNewContest(true))}>
          <Icon name="plus" /> 新建比赛
        </button>
      </div>
      {error ? <div className="empty">{error.message}</div> : data?.length === 0 ? (
        <div className="empty">还没有比赛。新建一场，然后在里面出题。</div>
      ) : (
        <div className="cards">
          {data?.map((c) => (
            <button key={c.id} className="card" onClick={() => navigate({ name: "studio", kind: "contest", id: c.id, editorial: false })}>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <h3>{c.meta.name}</h3>
                {c.meta.frozen && <span className="badge">已冻结</span>}
              </div>
              <div className="sub">{[c.meta.date, c.meta.location].filter(Boolean).join(" · ") || c.id}</div>
              <div className="letters">
                {c.meta.problem.map((p) => (
                  <span key={p.label} title={p.id}>{p.label}</span>
                ))}
                {c.meta.problem.length === 0 && <span className="empty-letter">未添加题目</span>}
              </div>
            </button>
          ))}
        </div>
      )}
      {newContest && (
        <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && setNewContest(false)}>
          <form className="dialog" onSubmit={create} onKeyDown={(e) => e.key === "Escape" && setNewContest(false)}>
            <label className="block-field">
              <span>比赛名称</span>
              <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="例如：2027 春季校赛" maxLength={80} />
            </label>
            {createError && <div className="error-text">{createError}</div>}
            <div className="dialog-actions">
              <button type="button" className="btn" onClick={() => setNewContest(false)}>
                取消
              </button>
              <button className="btn primary" disabled={!name.trim() || saving}>
                新建
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
