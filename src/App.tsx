import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauri } from '@tauri-apps/api/core';
import { api, errorMessage, slots, type DayView, type Preferences, type Slot, type TaskDraft, type TaskItem } from './api';
import { muteAudio, playCue, preloadSounds, primeAudio } from './feedback';
import s from './App.module.css';

function localDate(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}
function shiftDate(date: string, days: number) {
  const value = new Date(`${date}T12:00:00`);
  value.setDate(value.getDate() + days);
  return localDate(value);
}
function shortDate(date: string) { return date.slice(5).replace('-', '.'); }

type IconName = 'plus' | 'check' | 'arrow' | 'close' | 'sun' | 'sunset' | 'moon' | 'settings' | 'volume' | 'mute' | 'motion' | 'calendar' | 'chevron' | 'spark';
function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  const paths: Record<IconName, ReactNode> = {
    plus: <path d="M12 5v14M5 12h14" />,
    check: <path d="m5 12 4 4L19 6" />,
    arrow: <path d="M4 12h15m-6-6 6 6-6 6" />,
    close: <path d="m6 6 12 12M6 18 18 6" />,
    sun: <><circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1" /></>,
    sunset: <><path d="M3 17h18M5 21h14M6 14a6 6 0 0 1 12 0M12 3v3M3 7l2 2m14 0 2-2" /></>,
    moon: <path d="M20 15.5A9 9 0 0 1 8.5 4a9 9 0 1 0 11.5 11.5Z" />,
    settings: <><path d="m9 3-1 3-3 1v4l-2 2 2 3 3 1 1 3h6l1-3 3-1 2-3-2-2V7l-3-1-1-3Z" /><circle cx="12" cy="12" r="3" /></>,
    volume: <><path d="m11 4-6 5H2v6h3l6 5ZM16 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14" /></>,
    mute: <><path d="m11 4-6 5H2v6h3l6 5ZM16 9l6 6m-6 0 6-6" /></>,
    motion: <><path d="M3 6h11M1 12h8M3 18h11" /><path d="m13 8 6 4-6 4" /></>,
    calendar: <><rect x="3" y="5" width="18" height="16" rx="2" /><path d="M7 3v4m10-4v4M3 11h18m-13 5h1m6 0h1" /></>,
    chevron: <path d="m9 5 7 7-7 7" />,
    spark: <path d="m12 2 2.6 7.4L22 12l-7.4 2.6L12 22l-2.6-7.4L2 12l7.4-2.6Z" />,
  };
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}

function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    const cancel = (event: Event) => { event.preventDefault(); onClose(); };
    dialog.addEventListener('cancel', cancel);
    return () => { dialog.removeEventListener('cancel', cancel); dialog.close(); };
  }, [onClose]);
  return <dialog ref={ref} className={s.modal} aria-label={title}>
    <div className={s.modalHeader}><div><span className={s.eyebrow}>YOUR SPACE</span><h2>{title}</h2></div>
      <button className={s.iconButton} onClick={onClose} aria-label="关闭对话框"><Icon name="close" /></button></div>
    {children}
  </dialog>;
}

type Editor = { kind: 'create'; slot: Slot } | { kind: 'edit'; planId: string };

export default function App() {
  const [date, setDate] = useState(localDate);
  const [day, setDay] = useState<DayView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [filter, setFilter] = useState<Slot | 'all'>('all');
  const [hideCompleted, setHideCompleted] = useState(false);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [preferences, setPreferences] = useState<Preferences | null>(null);
  const [showPreferences, setShowPreferences] = useState(false);
  const [systemReduced, setSystemReduced] = useState(() => matchMedia('(prefers-reduced-motion: reduce)').matches);
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [dirty, setDirty] = useState(false);
  const [exitRequested, setExitRequested] = useState(false);
  const pendingRef = useRef(new Map<string, Promise<unknown>>());
  const guardRef = useRef({ dirty: false, pending: false });
  const saveEditorRef = useRef<(() => Promise<boolean>) | null>(null);
  const loadSequence = useRef(0);
  const dateRef = useRef(date);
  dateRef.current = date;
  guardRef.current = { dirty, pending: pending.size > 0 };
  const reduced = systemReduced || !!preferences?.reducedMotion;

  const load = useCallback(async (value: string) => {
    const sequence = ++loadSequence.current;
    setLoading(true);
    try {
      const result = await api.loadDay(value);
      if (sequence === loadSequence.current) { setDay(result); setPreferences(result.preferences); setError(null); }
    } catch (error) { if (sequence === loadSequence.current) setError(errorMessage(error)); }
    finally { if (sequence === loadSequence.current) setLoading(false); }
  }, []);
  useEffect(() => { void load(date); }, [date, load]);
  useEffect(() => { preloadSounds(); }, []);
  useEffect(() => { muteAudio(!preferences?.soundEnabled); }, [preferences?.soundEnabled]);
  useEffect(() => {
    const media = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setSystemReduced(media.matches);
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, []);
  useEffect(() => {
    let previousToday = localDate();
    const update = () => {
      const today = localDate();
      if (today !== previousToday) {
        if (dateRef.current === previousToday && !guardRef.current.dirty && !guardRef.current.pending) { setEditor(null); setDate(today); }
        previousToday = today;
      }
    };
    const interval = window.setInterval(update, 30_000);
    window.addEventListener('focus', update);
    return () => { clearInterval(interval); window.removeEventListener('focus', update); };
  }, []);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested(event => {
      if (guardRef.current.dirty || guardRef.current.pending) {
        event.preventDefault();
        setExitRequested(true);
      }
    }).then(cleanup => { if (disposed) cleanup(); else unlisten = cleanup; });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'n') {
        event.preventDefault();
        if (!guardRef.current.dirty) setEditor({ kind: 'create', slot: filter === 'all' ? 'morning' : filter });
      }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [filter]);
  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 3500);
    return () => clearTimeout(timeout);
  }, [notice]);

  async function mutate<T>(key: string, operation: () => Promise<T>): Promise<T | null> {
    if (pendingRef.current.has(key)) return null;
    const promise = operation();
    pendingRef.current.set(key, promise);
    setPending(new Set(pendingRef.current.keys()));
    try { const result = await promise; setError(null); return result; }
    catch (error) { setError(errorMessage(error)); return null; }
    finally { pendingRef.current.delete(key); setPending(new Set(pendingRef.current.keys())); }
  }
  function applyItem(item: TaskItem) {
    setDay(current => {
      if (!current || current.date !== item.scheduledOn) return current;
      const exists = current.items.some(existing => existing.planId === item.planId);
      return { ...current, items: exists ? current.items.map(existing => existing.planId === item.planId ? item : existing)
        : [...current.items, item].sort((a, b) => a.position - b.position) };
    });
  }
  async function complete(item: TaskItem, button: HTMLButtonElement | null) {
    if (!day) return;
    const desired = !item.completed;
    const result = await mutate(item.questId, () => api.setCompletion(day.datasetId, item, desired, crypto.randomUUID()));
    if (!result) return;
    applyItem(result.item);
    if (result.changed) {
      if (preferences?.soundEnabled) void playCue(desired ? 'complete' : 'undo');
      if (!reduced && button?.isConnected) button.animate([
        { transform: 'rotate(-9deg) scale(1)' },
        { transform: 'rotate(-9deg) scale(1.32)', offset: 0.35, boxShadow: '0 0 28px #39c7ff88' },
        { transform: 'rotate(-9deg) scale(1)' },
      ], { duration: 420, easing: 'cubic-bezier(.2,.8,.2,1)' });
      setNotice(desired ? '完成了一件实在的事。' : '已撤销完成，安排保留。');
    }
  }
  async function saveTask(draft: TaskDraft, ids: { questId: string; planId: string }) {
    if (!day || !editor) return false;
    const current = editor.kind === 'edit' ? day.items.find(item => item.planId === editor.planId) : undefined;
    if (editor.kind === 'edit' && !current) return false;
    const result = await mutate(current?.questId ?? 'create', () => current
      ? api.editQuest(day.datasetId, current, draft) : api.createQuest(day.datasetId, draft, ids));
    if (!result) return false;
    applyItem(result.item);
    setNotice(current ? '修改已保存。' : draft.date === date ? '新的安排，已加入当日。' : `已安排至 ${shortDate(draft.date)}。`);
    return true;
  }
  async function move(item: TaskItem, slot: Slot) {
    if (!day) return;
    const result = await mutate(item.questId, () => api.editQuest(day.datasetId, item, {
      title: item.currentTitle, description: item.currentDescription, date: item.scheduledOn,
      slot, startTime: item.startTime, endTime: item.endTime,
    }));
    if (result) applyItem(result.item);
  }
  async function savePreference(key: 'soundEnabled' | 'reducedMotion', value: boolean) {
    if (!day || !preferences) return;
    const previous = preferences;
    const next = { ...preferences, [key]: value };
    setPreferences(next);
    const saved = await mutate('preferences', () => api.savePreferences(day.datasetId, next));
    setPreferences(saved ?? previous);
  }
  function openEditor(value: Editor) {
    if (dirty) { setNotice('请先保存或关闭当前编辑。'); return; }
    setEditor(value);
  }
  function navigateDate(value: string) {
    if (dirty || pending.size) { setNotice('请先保存当前编辑，或等待操作完成。'); return; }
    if (value === date) return;
    setEditor(null);
    setDate(value);
  }
  const closePreferences = useCallback(() => setShowPreferences(false), []);
  const cancelExit = useCallback(() => setExitRequested(false), []);
  async function exit(save: boolean) {
    if (save && dirty && !(await saveEditorRef.current?.())) { setExitRequested(false); return; }
    const results = await Promise.allSettled([...pendingRef.current.values()]);
    if (results.some(result => result.status === 'rejected')) { setExitRequested(false); return; }
    if (isTauri()) await getCurrentWindow().destroy();
  }

  const visibleDay = day?.date === date ? day : null;
  const items = visibleDay?.items ?? [];
  const completed = items.filter(item => item.completed).length;
  const selectedDate = new Date(`${date}T12:00:00`);
  const weekday = new Intl.DateTimeFormat('en-US', { weekday: 'short' }).format(selectedDate).toUpperCase();
  const currentItem = editor?.kind === 'edit' ? items.find(item => item.planId === editor.planId) : undefined;

  return <div className={s.app} data-reduced-motion={reduced} onPointerDownCapture={primeAudio} onKeyDownCapture={primeAudio}>
    <div className={s.titlebar} data-tauri-drag-region><span data-tauri-drag-region>LIFEISRPG — A DAY AT A TIME</span></div>
    <aside className={s.rail} aria-label="应用导航">
      <div className={s.wordmark}><span className={s.brandMark}>L<span>R</span></span><div>LIFE<span>IS</span>RPG<small>MAKE IT YOUR DAY.</small></div></div>
      <div className={s.railSection}>日 常 / DAILY</div>
      <button className={s.activeNav} onClick={() => { navigateDate(localDate()); setFilter('all'); }} aria-current="page"><Icon name="sun" /><span>Today<small>今日行动</small></span><span className={s.navDot} /></button>
      <div className={s.railFoot}>
        <button className={s.settingsButton} onClick={() => setShowPreferences(true)} disabled={!preferences}><Icon name="settings" /><span>声音与动效</span></button>
        <div className={s.privateNote}><span />仅在此设备保存<small>LifeIsRPG · 0.1</small></div>
      </div>
    </aside>

    <main className={`${s.workspace} ${editor ? s.withEditor : ''}`}>
      <header className={s.hero}>
        <div className={s.heroGrid} aria-hidden="true" /><div className={s.heroCut} aria-hidden="true" />
        <div className={s.heroTop}><span className={s.eyebrow}>YOUR LIFE. YOUR NEXT MOVE.</span><span className={s.localBadge}><span />本地 · 离线可用</span></div>
        <div className={s.heroBody}><div><div className={s.chapter}>DAILY / ACTION</div><h1>{date === localDate() ? 'TODAY' : date > localDate() ? 'UP NEXT' : 'THAT DAY'}<span className={s.titlePeriod}>.</span></h1><p>{date === localDate() ? '把想做的事，变成今天的行动。' : date > localDate() ? '为接下来的一天，留好位置。' : '回看这一天的安排与完成。'}</p></div>
          <div className={s.dateStamp}><span>{selectedDate.getFullYear()} / {weekday}</span><strong>{shortDate(date)}</strong><em>{new Intl.DateTimeFormat('zh-CN', { weekday: 'long' }).format(selectedDate)}</em></div></div>
        <div className={s.heroBottom}><div className={s.planAct}><b>PLAN</b><span>—</span><b className={s.act}>ACT</b><span>—</span><b>REVIEW</b></div><span className={s.heroSmall}>ONE THING AT A TIME <Icon name="arrow" size={16} /></span></div>
      </header>

      <section className={s.planner} aria-label="当日安排">
        <div className={s.plannerHeading}><div><span className={s.sectionKicker}>THE DAILY LINEUP</span><h2>{date === localDate() ? '今日安排' : '当日安排'}<span>{String(items.length).padStart(2, '0')}</span></h2></div>
          <div className={s.dateControls}><button className={s.iconButton} aria-label="前一天" onClick={() => { navigateDate(shiftDate(date, -1)); }}><span className={s.chevronBack}><Icon name="chevron" size={16} /></span></button><label className={s.dateInput}><Icon name="calendar" size={16} /><input aria-label="查看日期" type="date" value={date} onChange={event => { if (event.target.value) navigateDate(event.target.value); }} /></label><button className={s.iconButton} aria-label="后一天" onClick={() => { navigateDate(shiftDate(date, 1)); }}><Icon name="chevron" size={16} /></button>{date !== localDate() && <button className={s.todayButton} onClick={() => { navigateDate(localDate()); }}>今天</button>}</div>
        </div>

        <div className={s.toolbar}><div className={s.filters} aria-label="筛选时段">{[{ id: 'all', label: '全天' }, ...slots].map(slot => <button key={slot.id} aria-pressed={filter === slot.id} className={filter === slot.id ? s.filterActive : ''} onClick={() => setFilter(slot.id as Slot | 'all')}>{slot.label}</button>)}</div>
          <label className={s.hideCompleted}><input type="checkbox" checked={hideCompleted} onChange={event => setHideCompleted(event.target.checked)} /><span>隐藏已完成</span></label></div>

        {error && <div className={s.error} role="alert"><span>{error}</span><div><button onClick={() => void load(date)} disabled={pending.size > 0}>重新读取</button><button onClick={() => setError(null)} aria-label="关闭错误提示"><Icon name="close" size={15} /></button></div></div>}
        {loading ? <div className={s.loading} role="status"><span />正在读取这一天…</div> : !visibleDay ? <div className={s.startError}><Icon name="calendar" size={32} /><h3>暂时无法读取本地安排</h3><p>数据没有被覆盖。请重试读取。</p><button className={s.secondaryButton} onClick={() => void load(date)}>重新读取</button></div> : <>
          <div className={s.progressSummary}><span>{completed ? `已完成 ${completed} 项` : '今天的行动，由你安排'}</span><div className={s.progressTrack}><div style={{ width: items.length ? `${completed / items.length * 100}%` : '0%' }} /></div><b>{completed}<span> / {items.length}</span></b></div>
          <div className={s.groups}>{slots.filter(slot => filter === 'all' || slot.id === filter).map(slot => {
            const all = items.filter(item => item.slot === slot.id);
            const visible = all.filter(item => !hideCompleted || !item.completed);
            return <section key={slot.id} className={s.group} aria-label={`${slot.label}任务`}><div className={s.groupHeader}><div className={s.groupName}><span className={s.slotIcon}><Icon name={slot.id === 'morning' ? 'sun' : slot.id === 'afternoon' ? 'sunset' : 'moon'} size={20} /></span><h3>{slot.label}<small>{slot.english}</small></h3><span className={s.groupRule} /></div><span className={s.groupCount}>{all.length} 项</span><button className={s.addSlot} aria-label={`添加${slot.label}任务`} onClick={() => openEditor({ kind: 'create', slot: slot.id })}><Icon name="plus" size={18} /></button></div>
              <div className={s.taskList}>{visible.map((item, index) => <TaskRow key={item.planId} item={item} index={index} selected={editor?.kind === 'edit' && editor.planId === item.planId} busy={pending.has(item.questId) || (dirty && editor?.kind === 'edit' && editor.planId === item.planId)} today={visibleDay.today} onComplete={complete} onSelect={() => openEditor({ kind: 'edit', planId: item.planId })} onMove={move} />)}
                {!visible.length && <button className={s.emptySlot} onClick={() => openEditor({ kind: 'create', slot: slot.id })}><span className={s.emptyDiamond}><Icon name="plus" size={14} /></span><span>{all.length && hideCompleted ? `${all.length} 项已完成已隐藏` : `添加${slot.label}的安排`}<small>{all.length && hideCompleted ? '关闭「隐藏已完成」即可查看' : '留给一件你想推进的事'}</small></span><Icon name="arrow" size={17} /></button>}
              </div></section>;
          })}</div>
          <button className={s.newQuest} onClick={() => openEditor({ kind: 'create', slot: filter === 'all' ? 'morning' : filter })}><span><Icon name="plus" size={20} />添加新的行动</span><kbd>⌘ N</kbd></button>
        </>}
        <footer className={s.footer}><span className={s.saveState}><span className={pending.size ? s.savingDot : s.savedDot} />{pending.size ? '正在保存…' : error ? '操作未保存，请查看错误提示' : visibleDay ? '已保存到此设备' : '等待连接本地数据'}</span><span>不必完成一切，先推进一件。</span></footer>
      </section>
    </main>

    {editor && visibleDay && <TaskEditor key={editor.kind === 'edit' ? editor.planId : `new-${editor.slot}`} item={currentItem} initialDate={date} initialSlot={editor.kind === 'create' ? editor.slot : currentItem?.slot ?? 'morning'} today={visibleDay.today} busy={pending.has(currentItem?.questId ?? 'create')} onSave={saveTask} onClose={() => { setEditor(null); setDirty(false); }} onDirty={setDirty} saveRef={saveEditorRef} />}
    {notice && <div className={s.notice} role="status"><Icon name="spark" size={16} /><span>{notice}</span></div>}
    {showPreferences && preferences && <Modal title="声音与动效" onClose={closePreferences}><p className={s.modalIntro}>给每次行动，一点恰到好处的反馈。</p><div className={s.preferenceRows}>
      <label><Icon name={preferences.soundEnabled ? 'volume' : 'mute'} /><span><b>交互音效</b><small>完成与撤销时播放短音效</small></span><input type="checkbox" role="switch" aria-label="交互音效" checked={preferences.soundEnabled} disabled={pending.has('preferences')} onChange={event => void savePreference('soundEnabled', event.target.checked)} /></label>
      <label><Icon name="motion" /><span><b>减少动效</b><small>{systemReduced ? '系统已启用减少动效' : '保留清晰的状态，减少移动与缩放'}</small></span><input type="checkbox" role="switch" aria-label="减少动效" checked={preferences.reducedMotion} disabled={pending.has('preferences')} onChange={event => void savePreference('reducedMotion', event.target.checked)} /></label></div><p className={s.preferenceFoot}>偏好自动保存在此设备。动效同时遵循系统设置。</p></Modal>}
    {exitRequested && <Modal title="退出前，保存这次修改" onClose={cancelExit}><p className={s.modalIntro}>{pending.size ? '还有操作正在保存，完成后再退出。' : '当前编辑尚未保存。已保存的任务不会受影响。'}</p><div className={s.exitActions}><button className={s.primaryButton} disabled={pending.size > 0} onClick={() => void exit(true)}>保存并退出<Icon name="arrow" size={16} /></button><button className={s.secondaryButton} disabled={pending.size > 0} onClick={() => void exit(false)}>放弃草稿并退出</button><button className={s.textButton} onClick={cancelExit}>继续编辑</button></div></Modal>}
  </div>;
}

function TaskRow({ item, index, selected, busy, today, onComplete, onSelect, onMove }: {
  item: TaskItem; index: number; selected: boolean; busy: boolean; today: string;
  onComplete: (item: TaskItem, button: HTMLButtonElement | null) => Promise<void>;
  onSelect: () => void; onMove: (item: TaskItem, slot: Slot) => Promise<void>;
}) {
  const button = useRef<HTMLButtonElement>(null);
  return <article className={`${s.task} ${item.completed ? s.completed : ''} ${selected ? s.selectedTask : ''}`} data-task-title={item.title} data-plan-id={item.planId} aria-busy={busy}>
    <button ref={button} className={s.completeButton} aria-label={`${item.completed ? '撤销完成' : '完成任务'}：${item.title}`} aria-pressed={item.completed} disabled={busy} onClick={() => void onComplete(item, button.current)}><span>{busy ? <span className={s.spinner} /> : item.completed ? <Icon name="check" size={20} /> : <span className={s.checkDot} />}</span></button>
    <button className={s.taskContent} onClick={onSelect}><span className={s.taskTopLine}><span className={s.taskTitle}>{item.title}</span>{item.completed && <span className={s.doneLabel}>已完成</span>}</span><span className={s.taskMeta}>{item.startTime ? `${item.startTime}${item.endTime ? ` – ${item.endTime}` : ''}` : '时间自由'}<i />独立任务{item.completedOn && item.completedOn !== item.scheduledOn && <><i />于 {shortDate(item.completedOn)} 完成</>}</span></button>
    {!item.completed && item.scheduledOn >= today && <select className={s.moveSelect} aria-label={`调整「${item.title}」时段`} value={item.slot} disabled={busy} onChange={event => void onMove(item, event.target.value as Slot)}>{slots.map(slot => <option key={slot.id} value={slot.id}>{slot.label}</option>)}</select>}
    <span className={s.rowNumber}>{String(index + 1).padStart(2, '0')}</span><button className={s.taskMore} onClick={onSelect} aria-label={`查看「${item.title}」详情`}><Icon name="chevron" size={17} /></button>
  </article>;
}

function TaskEditor({ item, initialDate, initialSlot, today, busy, onSave, onClose, onDirty, saveRef }: {
  item?: TaskItem; initialDate: string; initialSlot: Slot; today: string; busy: boolean;
  onSave: (draft: TaskDraft, ids: { questId: string; planId: string }) => Promise<boolean>;
  onClose: () => void; onDirty: (dirty: boolean) => void;
  saveRef: React.RefObject<(() => Promise<boolean>) | null>;
}) {
  const [draft, setDraft] = useState<TaskDraft>(() => ({ title: item?.currentTitle ?? '', description: item?.currentDescription ?? '', date: item?.scheduledOn ?? initialDate, slot: item?.slot ?? initialSlot, startTime: item?.startTime ?? null, endTime: item?.endTime ?? null }));
  const [saved, setSaved] = useState(() => JSON.stringify(draft));
  const [showTimes, setShowTimes] = useState(!!item?.startTime);
  const [confirmClose, setConfirmClose] = useState(false);
  const form = useRef<HTMLFormElement>(null);
  const title = useRef<HTMLInputElement>(null);
  const ids = useRef({ questId: crypto.randomUUID(), planId: crypto.randomUUID() });
  const dirty = JSON.stringify(draft) !== saved;
  const timeReadOnly = !!item && (item.completed || item.scheduledOn < today);
  const timeError = draft.endTime && (!draft.startTime || draft.endTime <= draft.startTime);
  useEffect(() => { onDirty(dirty); }, [dirty, onDirty]);
  useEffect(() => { title.current?.focus(); }, []);
  function field<K extends keyof TaskDraft>(key: K, value: TaskDraft[K]) { setDraft(current => ({ ...current, [key]: value })); }
  async function save(): Promise<boolean> {
    if (busy || !form.current?.reportValidity() || timeError) return false;
    const captured = { ...draft };
    if (!(await onSave(captured, ids.current))) return false;
    setSaved(JSON.stringify(captured));
    onDirty(false);
    return true;
  }
  saveRef.current = save;
  async function submit(event: FormEvent) { event.preventDefault(); if (await save()) onClose(); }
  const requestClose = useCallback(() => { if (dirty) setConfirmClose(true); else onClose(); }, [dirty, onClose]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !document.querySelector('dialog[open]')) { event.preventDefault(); requestClose(); }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') { event.preventDefault(); form.current?.requestSubmit(); }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [requestClose]);
  return <aside className={s.editor} aria-label={item ? '任务详情' : '新建任务'}><div className={s.editorHeader}><span className={s.eyebrow}>{item ? 'QUEST DETAILS' : 'A NEW ACTION'}</span><button className={s.iconButton} aria-label="关闭任务详情" onClick={requestClose} disabled={busy}><Icon name="close" /></button></div>
    <div className={s.editorTitle}><span className={s.editorDiamond}><Icon name={item?.completed ? 'check' : 'spark'} size={23} /></span><h2>{item ? '任务详情' : '新的行动'}<small>{item ? item.completed ? '已完成的事实，值得留下。' : '把下一步想清楚。' : '从一件具体的事开始。'}</small></h2></div>
    <form ref={form} onSubmit={event => void submit(event)} className={s.editorForm}><label>任务标题<input ref={title} required aria-label="任务标题" value={draft.title} placeholder="今天，想推进什么？" onChange={event => field('title', event.target.value)} disabled={busy} /></label>
      <label>补充说明<span className={s.optional}>可选</span><textarea aria-label="补充说明" rows={4} value={draft.description} onChange={event => field('description', event.target.value)} placeholder="写下范围、下一步，或需要记住的小事。" disabled={busy} /></label>
      <div className={s.formDivider} />
      <label>安排日期<input type="date" aria-label="安排日期" required value={draft.date} readOnly={!!item} onChange={event => field('date', event.target.value)} disabled={busy} /></label>
      <fieldset className={s.slotField} disabled={busy || timeReadOnly}><legend>时间段</legend><div>{slots.map(slot => <button type="button" key={slot.id} aria-pressed={draft.slot === slot.id} className={draft.slot === slot.id ? s.chosenSlot : ''} onClick={() => field('slot', slot.id)}><Icon name={slot.id === 'morning' ? 'sun' : slot.id === 'afternoon' ? 'sunset' : 'moon'} size={17} />{slot.label}</button>)}</div></fieldset>
      <button type="button" className={s.timeDisclosure} aria-expanded={showTimes} disabled={busy || timeReadOnly} onClick={() => { setShowTimes(!showTimes); if (showTimes) setDraft(current => ({ ...current, startTime: null, endTime: null })); }}><span><Icon name="calendar" size={16} />安排具体时间<span className={s.optional}>可选</span></span><span>{showTimes ? '−' : '+'}</span></button>
      {showTimes && <div className={s.timeFields}><label>开始<input aria-label="开始时间" type="time" value={draft.startTime ?? ''} onChange={event => field('startTime', event.target.value || null)} disabled={busy || timeReadOnly} /></label><span>—</span><label>结束<span className={s.optional}>可选</span><input aria-label="结束时间" type="time" value={draft.endTime ?? ''} onChange={event => field('endTime', event.target.value || null)} disabled={busy || timeReadOnly} /></label></div>}
      {timeError && <p className={s.fieldError} role="alert">结束时间须晚于开始时间，且在同一天内。</p>}
      <p className={s.formHint}>{timeReadOnly ? '原安排与完成记录保留当时的内容。任务标题和说明仍可修改。' : '时间只用于日程展示与规划。'}</p>
      {confirmClose && <div className={s.unsavedPrompt}><strong>修改尚未保存</strong><p>保存修改，或放弃这次编辑。</p><button type="button" className={s.secondaryButton} onClick={onClose} disabled={busy}>放弃修改</button><button type="button" className={s.textButton} onClick={() => setConfirmClose(false)}>继续编辑</button></div>}
      <div className={s.editorActions}><span>{busy ? '正在保存…' : dirty ? '有未保存的修改' : item ? '已保存' : '填写后即可保存'}</span><button type="submit" className={s.primaryButton} disabled={busy || !!timeError}>{busy ? '正在保存' : item ? '保存修改' : '加入安排'}<Icon name="arrow" size={17} /></button></div>
    </form></aside>;
}
