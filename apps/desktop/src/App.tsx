import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode, RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import notesData from "./data/notes.json";
import type { EventRow, HostInfo, Note } from "./types";
import {
  AlertIcon,
  BranchIcon,
  CheckIcon,
  ChevronIcon,
  CircleIcon,
  EllipsisIcon,
  FileIcon,
  GridIcon,
  HistoryIcon,
  LinkIcon,
  MicIcon,
  PanelIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  SendIcon,
  SettingsIcon,
  SparkleIcon,
  TargetIcon,
  UserIcon,
} from "./icons";

const fallbackNotes = notesData as Note[];

type Tab = "overview" | "understanding" | "search" | "practice";

const TABS: { id: Tab; label: string; icon: React.ReactNode }[] = [
  { id: "overview", label: "总览", icon: <TargetIcon size={13} /> },
  { id: "understanding", label: "理解", icon: <CircleIcon size={13} /> },
  { id: "search", label: "探索", icon: <SearchIcon size={13} /> },
  { id: "practice", label: "实践", icon: <PencilIcon size={13} /> },
];

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function nowLabel(): string {
  const now = new Date();
  return `今天 ${now.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`;
}

function SectionLabel({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <div className="section-label">
      <span className="section-label__icon">{icon}</span>
      <span>{children}</span>
    </div>
  );
}

function Rail() {
  return (
    <nav className="rail" aria-label="主导航">
      {!isTauri() ? (
        <div className="traffic-lights" aria-hidden="true">
          <span className="traffic-lights__dot traffic-lights__dot--close" />
          <span className="traffic-lights__dot traffic-lights__dot--min" />
          <span className="traffic-lights__dot traffic-lights__dot--zoom" />
        </div>
      ) : null}
      <div className="rail__traffic-space" />
      <button className="rail__button rail__button--active" title="工作线" aria-label="工作线">
        <GridIcon size={18} />
      </button>
      <button className="rail__button" title="搜索" aria-label="搜索">
        <SearchIcon size={18} />
      </button>
      <button className="rail__button" title="新建" aria-label="新建">
        <PlusIcon size={18} />
      </button>
      <button className="rail__button" title="文档" aria-label="文档">
        <FileIcon size={18} />
      </button>
      <button className="rail__button" title="链接" aria-label="链接">
        <LinkIcon size={18} />
      </button>
      <div className="rail__spacer" />
      <button className="rail__button" title="设置" aria-label="设置">
        <SettingsIcon size={18} />
      </button>
      <button className="rail__button" title="账户" aria-label="账户">
        <UserIcon size={18} />
      </button>
    </nav>
  );
}

function Sidebar({
  notes,
  selectedId,
  query,
  onQuery,
  onSelect,
  searchRef,
  host,
}: {
  notes: Note[];
  selectedId: string;
  query: string;
  onQuery: (value: string) => void;
  onSelect: (id: string) => void;
  searchRef: RefObject<HTMLInputElement | null>;
  host: HostInfo | null;
}) {
  return (
    <aside className="sidebar">
      <div className="sidebar__top">
        <label className="search-field">
          <SearchIcon size={14} />
          <input
            ref={searchRef}
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            placeholder="搜索工作线"
            aria-label="搜索工作线"
          />
        </label>
        <button className="new-line" type="button">
          <PlusIcon size={15} />
          <span>新建</span>
        </button>
      </div>
      <div className="line-list" role="listbox" aria-label="工作线列表">
        {notes.length === 0 ? <p className="line-list__empty">没有匹配的工作线</p> : null}
        {notes.map((note) => (
          <button
            key={note.id}
            type="button"
            role="option"
            aria-selected={note.id === selectedId}
            className={`line-row${note.id === selectedId ? " line-row--active" : ""}`}
            onClick={() => onSelect(note.id)}
          >
            <span className="line-row__head">
              <span className="line-row__title">{note.title}</span>
              <span className="line-row__date">{note.updated}</span>
            </span>
            <span className="line-row__next">下一步：{note.nextStep}</span>
          </button>
        ))}
      </div>
      <div className="sidebar__foot">
        <span className={`host-dot${host ? " host-dot--online" : ""}`} />
        <span>{host ? "本地宿主已连接" : "预览数据"}</span>
      </div>
    </aside>
  );
}

function Overview({ note }: { note: Note }) {
  const status = note.status;
  return (
    <article className="note">
      <div className="note__kind">
        <SparkleIcon size={13} />
        <span>{note.kind}</span>
      </div>
      <div className="note__title-row">
        <h1>{note.title}</h1>
        <button className="icon-button" type="button" title="重命名" aria-label="重命名">
          <PencilIcon size={15} />
        </button>
      </div>
      <p className="note__domains">{note.domains}</p>
      <p className="note__intro">{note.intro}</p>

      <section className="note-section">
        <SectionLabel icon={<TargetIcon size={15} />}>目标</SectionLabel>
        <p className="note-section__placeholder">{note.goal}</p>
      </section>

      <section className="note-section">
        <SectionLabel icon={<TargetIcon size={15} />}>当前状态</SectionLabel>
        <div className="status-cols">
          <div className="status-col">
            <strong>{status.proven}</strong>
            <span>已证明能做的</span>
          </div>
          <div className="status-col">
            <strong>{status.unstable}</strong>
            <span>还不稳的</span>
          </div>
          <div className="status-col">
            <strong>{status.discarded}</strong>
            <span>试过但废的</span>
          </div>
        </div>
        <div className="status-rule" />
        <p className="note-muted">
          实践 {status.practice} · 反对 {status.opposed} · 无法判断 {status.unknown}
        </p>
        <p className="note-muted">{status.evidence}</p>
      </section>

      <section className="note-section">
        <SectionLabel icon={<CircleIcon size={15} />}>下一步练什么</SectionLabel>
        <ul className="note-bullets">
          {note.nextSteps.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ul>
      </section>

      <section className="note-section">
        <SectionLabel icon={<CircleIcon size={15} />}>材料与约束</SectionLabel>
        <ul className="note-bullets note-bullets--tight">
          {note.materials.map((material) => (
            <li key={material}>{material}</li>
          ))}
        </ul>
      </section>

      <section className="note-section">
        <SectionLabel icon={<CheckIcon size={15} />}>已证明能做的</SectionLabel>
        <ul className="note-bullets">
          {note.proven.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </section>

      <section className="note-section">
        <SectionLabel icon={<AlertIcon size={15} />}>还不稳的</SectionLabel>
        <ul className="note-bullets">
          {note.unstable.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
        <button className="text-link" type="button">在理解页看全部 {note.unstable.length} 条</button>
      </section>

      <footer className="note-footer">
        <button className="text-link" type="button">
          <BranchIcon size={14} /> 谁改变了这条线
        </button>
        <button className="text-link" type="button">
          <HistoryIcon size={14} /> 修改历史（理解 · 探索 · 实践）
        </button>
      </footer>
    </article>
  );
}

function Understanding({ note }: { note: Note }) {
  return (
    <div className="subpage">
      <div className="subpage__head">
        <span className="note__kind"><SparkleIcon size={13} /> 理解</span>
        <h2>{note.title}</h2>
        <p>{note.intro}</p>
      </div>
      <div className="card-grid">
        <section className="panel-card">
          <SectionLabel icon={<TargetIcon size={15} />}>目标</SectionLabel>
          <p>{note.goal}</p>
        </section>
        <section className="panel-card">
          <SectionLabel icon={<CircleIcon size={15} />}>材料与约束</SectionLabel>
          <ul className="note-bullets note-bullets--tight">
            {note.materials.map((item) => <li key={item}>{item}</li>)}
          </ul>
        </section>
        <section className="panel-card">
          <SectionLabel icon={<CheckIcon size={15} />}>已证明能做的</SectionLabel>
          <ul className="note-bullets note-bullets--tight">
            {note.proven.map((item) => <li key={item}>{item}</li>)}
          </ul>
        </section>
        <section className="panel-card">
          <SectionLabel icon={<AlertIcon size={15} />}>还不稳的</SectionLabel>
          <ul className="note-bullets note-bullets--tight">
            {note.unstable.map((item) => <li key={item}>{item}</li>)}
          </ul>
        </section>
      </div>
    </div>
  );
}

function SearchView({ notes, query, onSelect }: { notes: Note[]; query: string; onSelect: (id: string) => void }) {
  return (
    <div className="subpage">
      <div className="subpage__head">
        <span className="note__kind"><SearchIcon size={13} /> 探索</span>
        <h2>{query ? `“${query}” 的结果` : "探索工作线"}</h2>
        <p>在标题、下一步与正文里查找。按 ⌘K 跳到搜索框。</p>
      </div>
      <div className="search-results">
        {notes.length === 0 ? <p className="note-muted">没有匹配的工作线。</p> : null}
        {notes.map((note) => (
          <button key={note.id} type="button" className="search-result" onClick={() => onSelect(note.id)}>
            <span className="search-result__title">{note.title}</span>
            <span className="search-result__meta">{note.updated} · {note.kind}</span>
            <span className="search-result__next">{note.nextStep}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

function Practice({ note, events }: { note: Note; events: EventRow[] }) {
  return (
    <div className="subpage">
      <div className="subpage__head">
        <span className="note__kind"><CheckIcon size={13} /> 实践</span>
        <h2>{note.title}</h2>
        <p>实践 {note.status.practice} · 反对 {note.status.opposed} · 无法判断 {note.status.unknown}</p>
      </div>
      <div className="practice-timeline">
        {events.length === 0 ? (
          <p className="note-muted">还没有实践记录。在右下角写下下一步并发送，事件会出现在这里。</p>
        ) : null}
        {events.map((event) => (
          <div key={event.id} className="practice-event">
            <span className="practice-event__time">{event.at}</span>
            <p>{event.text}</p>
          </div>
        ))}
      </div>
    </div>
  );
}

function Inspector({
  note,
  events,
  checks,
  onCheck,
  composer,
  onComposer,
  onSend,
  suggestionHidden,
  onFill,
  onDismiss,
  host,
}: {
  note: Note;
  events: EventRow[];
  checks: boolean[];
  onCheck: (index: number) => void;
  composer: string;
  onComposer: (value: string) => void;
  onSend: () => void;
  suggestionHidden: boolean;
  onFill: () => void;
  onDismiss: () => void;
  host: HostInfo | null;
}) {
  const items = [
    { text: note.agent.request, detail: "不要发这条" },
    { text: note.agent.independent, detail: "独立执行" },
    { text: note.agent.sources, detail: "只引用公开证据" },
  ];
  return (
    <aside className="inspector" aria-label="智能体上下文">
      <header className="inspector__head">
        <span>智能体将收到的内容</span>
      </header>
      <div className="inspector__scroll">
        <ul className="check-list">
          {items.map((item, index) => (
            <li key={item.text} className="check-row">
              <button
                type="button"
                className={`check-box${checks[index] ? " check-box--checked" : ""}`}
                onClick={() => onCheck(index)}
                aria-pressed={checks[index]}
                aria-label={`选择：${item.text}`}
              >
                {checks[index] ? <CheckIcon size={11} /> : null}
              </button>
              <span className="check-row__body">
                <span>{item.text}</span>
                <em>{item.detail}</em>
              </span>
            </li>
          ))}
        </ul>
        <p className="agent-note">{note.agent.note}</p>
        <button className="accordion" type="button">
          <span>智能体已有的内容 {note.agent.existing}</span>
          <ChevronIcon size={13} />
        </button>
        <div className="activity">
          {events.length === 0 ? (
            <p className="activity__empty">还没有活动。发送后事件会在这里流入。</p>
          ) : (
            events.map((event) => (
              <div key={event.id} className="activity__item">
                <span>{event.at}</span>
                <p>{event.text}</p>
              </div>
            ))
          )}
        </div>
      </div>

      {!suggestionHidden ? (
        <div className="suggestion">
          <span className="suggestion__label">建议的下一步</span>
          <p>{note.agent.suggestion}</p>
          <div className="suggestion__actions">
            <button className="small-button" type="button" onClick={onFill}>填入</button>
            <button className="small-button small-button--quiet" type="button" onClick={onDismiss}>忽略</button>
          </div>
        </div>
      ) : null}

      <div className="composer">
        <textarea
          value={composer}
          onChange={(event) => onComposer(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              onSend();
            }
          }}
          rows={1}
          placeholder="写下下一步，然后发送。"
          aria-label="写下下一步"
        />
        <div className="composer__bar">
          <button className="composer__icon" type="button" title="语音" aria-label="语音">
            <MicIcon size={16} />
          </button>
          <span className="composer__hint">{host ? "本地 SQLite" : "预览"}</span>
          <button
            className="composer__send"
            type="button"
            onClick={onSend}
            disabled={composer.trim().length === 0}
            title="发送"
            aria-label="发送"
          >
            <SendIcon size={16} />
          </button>
        </div>
        <p className="composer__foot">
          这条线的工作文件放在应用数据目录；智能体与本机命令都在沙箱里读取。
        </p>
      </div>
    </aside>
  );
}

export default function App() {
  const [notes, setNotes] = useState<Note[]>(fallbackNotes);
  const [selectedId, setSelectedId] = useState(fallbackNotes[0]?.id ?? "");
  const [tab, setTab] = useState<Tab>("overview");
  const [query, setQuery] = useState("");
  const [host, setHost] = useState<HostInfo | null>(null);
  const [events, setEvents] = useState<Record<string, EventRow[]>>({});
  const [checks, setChecks] = useState([true, true, true]);
  const [composer, setComposer] = useState("");
  const [suggestionHidden, setSuggestionHidden] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const searchRef = useRef<HTMLInputElement>(null);

  const selected = useMemo(
    () => notes.find((note) => note.id === selectedId) ?? notes[0],
    [notes, selectedId],
  );

  const filteredNotes = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return notes;
    return notes.filter((note) =>
      [note.title, note.nextStep, note.intro, note.domains].join(" ").toLowerCase().includes(needle),
    );
  }, [notes, query]);

  useEffect(() => {
    if (!isTauri()) return;
    void invoke<Note[]>("notes_all")
      .then((loaded) => {
        if (loaded.length > 0) {
          setNotes(loaded);
          setSelectedId((current) => (loaded.some((note) => note.id === current) ? current : loaded[0].id));
        }
      })
      .catch(() => undefined);
    void invoke<HostInfo>("host_info").then(setHost).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!isTauri() || !selected) return;
    void invoke<EventRow[]>("events_list", { noteId: selected.id })
      .then((loaded) => setEvents((current) => ({ ...current, [selected.id]: loaded })))
      .catch(() => undefined);
  }, [selected?.id]);

  useEffect(() => {
    setChecks([true, true, true]);
    setSuggestionHidden(false);
  }, [selected?.id]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchRef.current?.focus();
        setTab("search");
      }
      if (event.key === "Escape") {
        searchRef.current?.blur();
        setQuery("");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const send = useCallback(() => {
    const text = composer.trim();
    if (!text || !selected) return;
    const local: EventRow = { id: Date.now(), at: nowLabel(), text };
    setEvents((current) => ({
      ...current,
      [selected.id]: [...(current[selected.id] ?? []), local],
    }));
    setComposer("");
    if (isTauri()) {
      void invoke<EventRow>("event_append", { noteId: selected.id, text }).catch(() => undefined);
    }
  }, [composer, selected]);

  if (!selected) {
    return <div className="empty-shell">没有可显示的工作线。</div>;
  }

  return (
    <div className={`app${inspectorOpen ? "" : " app--inspector-hidden"}`}>
      <Rail />
      <Sidebar
        notes={filteredNotes}
        selectedId={selected.id}
        query={query}
        onQuery={(value) => {
          setQuery(value);
          if (value) setTab("search");
        }}
        onSelect={(id) => {
          setSelectedId(id);
          setTab("overview");
        }}
        searchRef={searchRef}
        host={host}
      />
      <main className="main">
        <header className="topbar">
          <div className="tabs" role="tablist" aria-label="视图">
            {TABS.map((item) => (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={tab === item.id}
                className={`tab${tab === item.id ? " tab--active" : ""}`}
                onClick={() => setTab(item.id)}
              >
                {item.icon}
                {item.label}
              </button>
            ))}
          </div>
          <div className="topbar__actions">
            <button className="icon-button" type="button" title="更多" aria-label="更多">
              <EllipsisIcon size={16} />
            </button>
            <button
              className="icon-button"
              type="button"
              title="切换检查器"
              aria-label="切换检查器"
              onClick={() => setInspectorOpen((open) => !open)}
            >
              <PanelIcon size={16} />
            </button>
          </div>
        </header>
        <div className="main__scroll">
          {tab === "overview" ? <Overview note={selected} /> : null}
          {tab === "understanding" ? <Understanding note={selected} /> : null}
          {tab === "search" ? <SearchView notes={filteredNotes} query={query} onSelect={(id) => { setSelectedId(id); setTab("overview"); }} /> : null}
          {tab === "practice" ? <Practice note={selected} events={events[selected.id] ?? []} /> : null}
        </div>
      </main>
      {inspectorOpen ? (
        <Inspector
          note={selected}
          events={events[selected.id] ?? []}
          checks={checks}
          onCheck={(index) => setChecks((current) => current.map((value, at) => (at === index ? !value : value)))}
          composer={composer}
          onComposer={setComposer}
          onSend={send}
          suggestionHidden={suggestionHidden}
          onFill={() => setComposer(selected.agent.suggestion)}
          onDismiss={() => setSuggestionHidden(true)}
          host={host}
        />
      ) : null}
    </div>
  );
}
