import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { api, slots, type DeletePreview, type Execution, type Goal, type Quest, type QuestDraft, type QuestHistory, type RunMutation, type Slot, type TaskItem, type Workspace } from './api';
import { Icon, Modal } from './ui';
import { playCue } from './feedback';
import s from './App.module.css';

export type SaveRef = React.RefObject<(() => Promise<boolean>) | null>;
interface EditorProps { data: Workspace; run: RunMutation; busy: boolean; onChanged: () => Promise<void>; onClose: () => void; onDirty: (dirty: boolean) => void; saveRef: SaveRef }

function EditorFrame({ title, children, after, dirty, busy, onSave, onClose, onDirty, saveRef }: { title: string; children: ReactNode; after?: ReactNode; dirty: boolean; busy: boolean; onSave: () => Promise<boolean>; onClose: () => void; onDirty: (v: boolean) => void; saveRef: SaveRef }) {
  const form = useRef<HTMLFormElement>(null);
  const [discard, setDiscard] = useState(false);
  const close = useCallback(() => { if (dirty) setDiscard(true); else onClose(); }, [dirty, onClose]);
  useEffect(() => { onDirty(dirty); return () => onDirty(false); }, [dirty, onDirty]);
  async function save() { if (busy || !form.current?.reportValidity()) return false; return onSave(); }
  saveRef.current = save;
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (document.querySelector('dialog[open]')) return;
      if (e.key === 'Escape') { e.preventDefault(); if (!busy) close(); }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') { e.preventDefault(); form.current?.requestSubmit(); }
    };
    window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key);
  }, [close, busy]);
  return <aside className={s.editor} aria-label={title}>
    <div className={s.editorHeader}><span className={s.eyebrow}>MAKE IT REAL</span><button className={s.iconButton} aria-label="关闭任务详情" disabled={busy} onClick={close}><Icon name="close" /></button></div>
    <div className={s.editorTitle}><span className={s.editorDiamond}><Icon name="spark" /></span><h2>{title}<small>每一步，都留下真实的痕迹。</small></h2></div>
    <form ref={form} className={s.editorForm} onSubmit={e => { e.preventDefault(); void save().then(ok => { if (ok) onClose(); }); }}>
      <fieldset className={s.formBody} disabled={busy}>{children}</fieldset>
      {discard && <div className={s.unsavedPrompt}><strong>修改尚未保存</strong><p>保存修改，或放弃这次编辑。</p><button type="button" className={s.secondaryButton} onClick={onClose}>放弃修改</button><button type="button" className={s.textButton} onClick={() => setDiscard(false)}>继续编辑</button></div>}
      <div className={s.editorActions}><span>{busy ? '正在保存…' : dirty ? '有未保存的修改' : '已保存'}</span><button type="submit" className={s.primaryButton} disabled={busy}>保存修改<Icon name="arrow" size={16} /></button></div>
    </form>{after}
  </aside>;
}

export function QuestLinks({ data, questId, draft, onChange }: { data: Workspace; questId?: string; draft: QuestDraft; onChange: (draft: QuestDraft) => void }) {
  const descendants = new Set<string>(questId ? [questId] : []);
  for (let i = 0; i < data.quests.length; i++) for (const q of data.quests) if (q.parentId && descendants.has(q.parentId)) descendants.add(q.id);
  return <>
    <label>父任务<select aria-label="父任务" value={draft.parentId ?? ''} onChange={e => onChange({ ...draft, parentId: e.target.value || null })}><option value="">独立任务（顶层）</option>{data.quests.filter(q => !descendants.has(q.id) && q.mode === 'single').map(q => <option key={q.id} value={q.id}>{q.title}{q.archivedOn ? ' · 已归档' : ''}</option>)}</select></label>
    <fieldset className={s.goalChoices}><legend>关联目标 <span>可多选</span></legend>{data.goals.map(g => <label key={g.id}><input type="checkbox" aria-label={`关联目标：${g.name}`} checked={draft.goalIds.includes(g.id)} onChange={e => onChange({ ...draft, goalIds: e.target.checked ? [...draft.goalIds, g.id] : draft.goalIds.filter(id => id !== g.id) })} /><span>{g.name}<small>{g.kind === 'main' ? '主线' : '支线'}{g.archived ? ' · 已归档' : ''}</small></span></label>)}{!data.goals.length && <p className={s.formHint}>可以先创建任务，之后再到 Quests 添加目标。</p>}</fieldset>
  </>;
}

export function Quests({ data, onQuest, onCreate, onGoal, selected }: { data: Workspace; onQuest: (id: string) => void; onCreate: () => void; onGoal: (goal: Goal | null) => void; selected?: string }) {
  const [filter, setFilter] = useState('active');
  const [goal, setGoal] = useState('');
  const [search, setSearch] = useState('');
  const [archivedGoals, setArchivedGoals] = useState(false);
  const visible = data.quests.filter(q => (filter === 'archived' ? !!q.archivedOn : !q.archivedOn && (filter === 'active' ? q.status === 'open' : filter === 'all' || q.status === filter)) && (!goal || q.goalIds.includes(goal)) && q.title.toLowerCase().includes(search.toLowerCase()));
  const depth = (q: Quest) => { let n = 0; let p = q.parentId; const seen = new Set([q.id]); while (p && !seen.has(p)) { seen.add(p); n++; p = data.quests.find(q => q.id === p)?.parentId ?? null; } return Math.min(n, 4); };
  return <section className={s.questPage}>
    <header className={s.collectionHero}><span className={s.eyebrow}>DIRECTION / NEXT ACTION</span><h1>QUESTS<span>.</span></h1><p>目标给出方向，任务承接下一步。</p></header>
    <div className={s.collectionHeading}><h2>主线与支线</h2><button className={s.textButton} onClick={() => setArchivedGoals(!archivedGoals)}>{archivedGoals ? '查看活跃目标' : '查看归档目标'}</button><button className={s.secondaryButton} onClick={() => onGoal(null)}>＋ 新建目标</button></div>
    <div className={s.goalCards}>{data.goals.filter(g => g.archived === archivedGoals).map(g => <button className={s.goalCard} key={g.id} data-goal-name={g.name} onClick={() => onGoal(g)}><span>{g.kind === 'main' ? 'MAIN / 主线' : 'SIDE / 支线'}</span><h3>{g.name}</h3><p>{g.description || '一步一步，靠近想要的结果。'}</p><small>{g.status === 'completed' ? '已确认达成' : '开放目标'} · {data.quests.filter(q => q.goalIds.includes(g.id) && !q.archivedOn).length} 项任务</small></button>)}{!data.goals.some(g => g.archived === archivedGoals) && <p className={s.collectionEmpty}>{archivedGoals ? '还没有归档的目标。' : '可以先做一件事，也可以先给它一个方向。'}</p>}</div>
    <div className={s.collectionHeading}><h2>任务库 <small>{visible.length}</small></h2><button className={s.primaryButton} onClick={onCreate}>＋ 新建任务</button></div>
    <div className={s.questFilters}><input aria-label="搜索任务" placeholder="搜索任务…" value={search} onChange={e => setSearch(e.target.value)} /><select aria-label="任务状态筛选" value={filter} onChange={e => setFilter(e.target.value)}><option value="active">未完成</option><option value="all">全部未归档</option><option value="completed">已完成</option><option value="cancelled">已取消</option><option value="archived">已归档</option></select><select aria-label="按目标筛选" value={goal} onChange={e => setGoal(e.target.value)}><option value="">全部目标</option>{data.goals.map(g => <option key={g.id} value={g.id}>{g.name}</option>)}</select></div>
    <div className={s.questRows}>{visible.map(q => <button key={q.id} data-quest-title={q.title} className={`${s.questRow} ${selected === q.id ? s.questRowSelected : ''}`} onClick={() => onQuest(q.id)} style={{ marginLeft: depth(q) * 12 }}><span className={s.questStateIcon}>{q.status === 'completed' ? '✓' : q.status === 'cancelled' ? '−' : '◇'}</span><span><b>{q.title}</b><small>{q.parentId ? `子任务 · ${data.quests.find(p => p.id === q.parentId)?.title ?? '父任务'}` : '独立任务'}{q.goalIds.length ? ` · ${q.goalIds.map(id => data.goals.find(g => g.id === id)?.name).join(' / ')}` : ' · 无目标归属'}</small></span><em>{q.archivedOn ? '已归档' : q.status === 'completed' ? '已完成' : q.status === 'cancelled' ? '已取消' : '未完成'}</em><Icon name="chevron" size={16} /></button>)}{!visible.length && <div className={s.collectionEmpty}>此筛选下没有任务。任务可以不安排日期，也可以不关联目标。</div>}</div>
  </section>;
}

export function GoalEditor({ goal, ...props }: EditorProps & { goal: Goal | null }) {
  const [draft, setDraft] = useState<Goal>(() => goal ?? { id: crypto.randomUUID(), name: '', description: '', kind: 'main', status: 'open', archived: false, revision: 0 });
  const original = useRef(JSON.stringify(draft));
  const save = async () => { const result = await props.run(draft.id, () => api.saveGoal(props.data.datasetId, draft, goal?.revision ?? null)); if (!result) return false; await props.onChanged(); return true; };
  return <EditorFrame {...props} title={goal ? '目标详情' : '新建目标'} dirty={JSON.stringify(draft) !== original.current} onSave={save}>
    <label>目标名称<input autoFocus required aria-label="目标名称" value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} /></label>
    <label>目标说明<textarea aria-label="目标说明" rows={4} value={draft.description} onChange={e => setDraft({ ...draft, description: e.target.value })} /></label>
    <label>目标类别<select aria-label="目标类别" value={draft.kind} onChange={e => setDraft({ ...draft, kind: e.target.value as Goal['kind'] })}><option value="main">主线</option><option value="side">支线</option></select></label>
    <label className={s.checkLabel}><input type="checkbox" aria-label="确认目标达成" checked={draft.status === 'completed'} onChange={e => setDraft({ ...draft, status: e.target.checked ? 'completed' : 'open' })} />我确认这个目标已经达成</label><p className={s.formHint}>任务和子任务的完成不会自动结束目标。</p>
    <label className={s.checkLabel}><input type="checkbox" aria-label="归档目标" checked={draft.archived} onChange={e => setDraft({ ...draft, archived: e.target.checked })} />归档目标</label>
  </EditorFrame>;
}

export function RecordList({ records, onQuest }: { records: Execution[]; onQuest?: (id: string) => void }) {
  return <div className={s.recordList}>{records.map(record => <article key={record.id} data-record-title={record.snapshot.title} className={record.revokedAt ? s.revokedRecord : ''}><div><b>{record.kind === 'progress' ? '有进展' : record.revokedAt ? '已撤销完成' : '已完成'}</b><span>{record.occurredOn}</span></div><h4>{onQuest ? <button type="button" onClick={() => onQuest(record.questId)}>{record.snapshot.title}</button> : record.snapshot.title}</h4>{record.note && <p>{record.note}</p>}<small>{record.snapshot.goals.map(g => g.name).join(' / ') || '无目标归属'} · {record.planId ? '关联原计划' : '无预先计划'}<br />录入于 {new Date(record.recordedAt).toLocaleString('zh-CN')}{record.revokedAt && ` · 撤销于 ${new Date(record.revokedAt).toLocaleString('zh-CN')}`}</small></article>)}</div>;
}

export function QuestEditor({ quest, plan, initialParent, onQuest, onCreateChild, onExecution, onSchedule, onDelete, ...props }: EditorProps & { quest: Quest | null; plan?: TaskItem; initialParent?: string; onQuest: (id: string) => void; onCreateChild: (id: string) => void; onExecution: (kind: 'progress' | 'completion' | 'undo') => void; onSchedule: () => void; onDelete: () => void }) {
  const [draft, setDraft] = useState<QuestDraft>(() => ({ title: quest?.title ?? '', description: quest?.description ?? '', parentId: quest?.parentId ?? initialParent ?? null, goalIds: quest?.goalIds ?? [] }));
  const [time, setTime] = useState(() => ({ slot: plan?.slot ?? 'morning', startTime: plan?.startTime ?? '', endTime: plan?.endTime ?? '' }));
  const original = useRef(JSON.stringify({ draft, time }));
  const id = useRef(quest?.id ?? crypto.randomUUID());
  const baseRevision = useRef(quest?.revision ?? null);
  const basePlan = useRef(plan);
  const [history, setHistory] = useState<QuestHistory | null>(null);
  const [historyError, setHistoryError] = useState(false);
  const dirty = JSON.stringify({ draft, time }) !== original.current;
  useEffect(() => { if (!quest) return; let cancelled = false; void api.history(quest.id).then(value => { if (!cancelled) setHistory(value); }).catch(() => { if (!cancelled) setHistoryError(true); }); return () => { cancelled = true; }; }, [quest?.id, quest?.revision]);
  const save = async () => {
    const result = await props.run<unknown>(id.current, () => basePlan.current
      ? api.editQuest(props.data.datasetId, basePlan.current, { ...draft, date: basePlan.current.scheduledOn, ...time, startTime: time.startTime || null, endTime: time.endTime || null })
      : api.saveQuest(props.data.datasetId, id.current, draft, baseRevision.current));
    if (!result) return false; await props.onChanged(); return true;
  };
  const actionable = !dirty && !props.busy;
  const state = async (action: 'archive' | 'unarchive' | 'cancel' | 'reopen') => {
    if (!quest) return;
    if (!await props.run(quest.id, () => api.questState(props.data.datasetId, quest, action))) return;
    await props.onChanged(); props.onClose();
  };
  const timeReadOnly = !!plan && (quest?.status !== 'open' || plan.scheduledOn < props.data.today);
  const after = quest && <div className={s.questContext}>
    <div className={s.contextActions}><button type="button" disabled={!actionable || quest.status !== 'open' || !!quest.archivedOn} onClick={() => onExecution('progress')}>记录进展</button>{quest.status === 'completed' ? <button type="button" disabled={!actionable} onClick={() => onExecution('undo')}>撤销任务完成</button> : <button type="button" disabled={!actionable || quest.status !== 'open' || !!quest.archivedOn} onClick={() => onExecution('completion')}>完成整个任务</button>}
    <button type="button" disabled={!actionable || quest.status !== 'open' || !!quest.archivedOn} onClick={onSchedule}>添加日期安排</button></div>
    {dirty && <p className={s.formHint}>先保存或放弃修改，再记录执行或切换任务。</p>}
    <div className={s.contextHeading}><h3>子任务</h3><button type="button" className={s.textButton} disabled={!actionable || quest.mode !== 'single'} onClick={() => onCreateChild(quest.id)}>＋ 添加</button></div><p className={s.formHint}>父子任务独立完成，不自动重置或级联。</p>
    {props.data.quests.filter(q => q.parentId === quest.id).map(child => <button type="button" className={s.childRow} key={child.id} disabled={!actionable} onClick={() => onQuest(child.id)}><span>{child.status === 'completed' ? '✓' : '◇'} {child.title}</span><small>{child.archivedOn ? '已归档' : child.status === 'completed' ? '已完成' : '未完成'}</small></button>)}
    <h3>执行历史</h3>{historyError ? <p role="alert">历史读取失败，请关闭后重试。</p> : history ? history.records.length ? <RecordList records={history.records} /> : <p className={s.formHint}>还没有执行记录。安排与执行分别保存。</p> : <p className={s.formHint}>正在读取历史…</p>}
    {!!history?.plans.length && <details className={s.planHistory}><summary>计划历史 · {history.plans.length}</summary>{history.plans.map(p => <p key={p.id}>{p.date} · {slots.find(s => s.id === p.slot)?.label}<br />{p.title} · {p.disposition === 'active' ? '原安排保留' : p.disposition}</p>)}</details>}
    <div className={s.dangerActions}><button type="button" disabled={!actionable} onClick={() => void state(quest.archivedOn ? 'unarchive' : 'archive')}>{quest.archivedOn ? '恢复归档任务' : '归档任务'}</button>{quest.status !== 'completed' && <button type="button" disabled={!actionable} onClick={() => void state(quest.status === 'cancelled' ? 'reopen' : 'cancel')}>{quest.status === 'cancelled' ? '恢复未完成' : '取消任务'}</button>}<button type="button" disabled={!actionable} onClick={onDelete}>彻底删除任务</button></div>
  </div>;
  return <EditorFrame {...props} title={quest ? '任务详情' : '新建任务'} dirty={dirty} onSave={save} after={after}>
    <label>任务标题<input autoFocus required aria-label="任务标题" value={draft.title} onChange={e => setDraft({ ...draft, title: e.target.value })} /></label><label>补充说明<textarea aria-label="补充说明" rows={4} value={draft.description} onChange={e => setDraft({ ...draft, description: e.target.value })} /></label>
    <QuestLinks data={props.data} questId={quest?.id} draft={draft} onChange={setDraft} />
    {plan && <><label>安排日期<input aria-label="安排日期" type="date" readOnly value={plan.scheduledOn} /></label><label>时间段<select aria-label="时间段" disabled={timeReadOnly} value={time.slot} onChange={e => setTime({ ...time, slot: e.target.value as Slot })}>{slots.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}</select></label><div className={s.timeFields}><label>开始<input aria-label="开始时间" type="time" disabled={timeReadOnly} value={time.startTime} onChange={e => setTime({ ...time, startTime: e.target.value })} /></label><label>结束<input aria-label="结束时间" type="time" disabled={timeReadOnly} value={time.endTime} onChange={e => setTime({ ...time, endTime: e.target.value })} /></label></div></>}
    <p className={s.formHint}>{plan ? '编辑当前任务；已发生的执行和过去的安排保留原内容。' : '任务不需要先安排日期，也可以独立记录进展或完成。'}</p>
  </EditorFrame>;
}

export function ExecutionEditor({ quest, plan, kind, ...props }: EditorProps & { quest: Quest; plan?: TaskItem; kind: 'progress' | 'completion' | 'undo' }) {
  const [date, setDate] = useState(props.data.today);
  const [note, setNote] = useState('');
  const [confirm, setConfirm] = useState(false);
  const id = useRef(crypto.randomUUID());
  const originalQuest = useRef(quest);
  const originalPlan = useRef(plan);
  const save = async () => {
    if (!confirm) return false;
    const result = await props.run(quest.id, () => api.record(props.data.datasetId, originalQuest.current, { recordId: id.current, kind, occurredOn: date, note }, originalPlan.current));
    if (result === null) return false;
    if (result && kind !== 'progress') void playCue(kind === 'undo' ? 'undo' : 'complete');
    await props.onChanged(); return true;
  };
  return <EditorFrame {...props} title={kind === 'progress' ? '记录进展' : kind === 'undo' ? '撤销任务完成' : '完成整个任务'} dirty={!!note || confirm || date !== props.data.today} onSave={save}>
    <p className={s.executionTitle}>{plan?.title ?? quest.title}</p>
    {kind === 'undo' ? <p className={s.formHint}>撤销该任务当前有效的完成；之前独立记录的进展会保留。</p> : <><label>实际发生日期<input type="date" aria-label="实际发生日期" required max={props.data.today} value={date} onChange={e => setDate(e.target.value)} /></label><label>进展或完成说明<span className={s.optional}>可留空</span><textarea aria-label="执行说明" rows={4} value={note} onChange={e => setNote(e.target.value)} /></label><p className={s.formHint}>录入时间自动另存；事实出现在实际发生日期的足迹中。{plan ? '关联所选安排，保留该计划的上下文。' : '无预先计划：保存本次确认的当前任务标题和目标关联，不推断旧版本。'}</p><div className={s.snapshotPreview}><b>本次保存的上下文</b><p>{plan?.title ?? quest.title}</p><small>{plan ? plan.goals.map(g => g.name).join(' / ') || '无目标归属' : quest.goalIds.map(id => props.data.goals.find(g => g.id === id)?.name).join(' / ') || '无目标归属'}</small></div></>}
    <label className={s.checkLabel}><input type="checkbox" aria-label="确认执行内容" required checked={confirm} onChange={e => setConfirm(e.target.checked)} />{kind === 'undo' ? '确认撤销完成，保留独立进展' : kind === 'completion' ? '确认任务已经完成，保存以上事实' : '确认以上内容；任务仍未完成'}</label>
  </EditorFrame>;
}

export function ScheduleEditor({ quest, ...props }: EditorProps & { quest: Quest }) {
  const [draft, setDraft] = useState({ date: props.data.today, slot: 'morning' as Slot, startTime: '', endTime: '' });
  const original = useRef(JSON.stringify(draft));
  const id = useRef(crypto.randomUUID());
  const base = useRef(quest);
  const save = async () => {
    const result = await props.run(quest.id, () => api.schedule(props.data.datasetId, base.current, id.current, { ...draft, ...base.current, startTime: draft.startTime || null, endTime: draft.endTime || null }));
    if (!result) return false; await props.onChanged(); return true;
  };
  return <EditorFrame {...props} title="添加日期安排" dirty={original.current !== JSON.stringify(draft)} onSave={save}>
    <p className={s.executionTitle}>{quest.title}</p><label>安排日期<input required min={props.data.today} type="date" aria-label="安排日期" value={draft.date} onChange={e => setDraft({ ...draft, date: e.target.value })} /></label><label>时间段<select aria-label="时间段" value={draft.slot} onChange={e => setDraft({ ...draft, slot: e.target.value as Slot })}>{slots.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}</select></label><div className={s.timeFields}><label>开始<input type="time" aria-label="开始时间" value={draft.startTime} onChange={e => setDraft({ ...draft, startTime: e.target.value })} /></label><label>结束<input type="time" aria-label="结束时间" value={draft.endTime} onChange={e => setDraft({ ...draft, endTime: e.target.value })} /></label></div><p className={s.formHint}>为同一任务添加计划，保留已有日期与执行历史。时间仅用于展示和规划。</p>
  </EditorFrame>;
}

export function DeleteDialog({ preview, busy, onClose, onDelete }: { preview: DeletePreview; busy: boolean; onClose: () => void; onDelete: (confirmed: boolean) => void }) {
  const [second, setSecond] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  return <Modal title={second ? '再次确认彻底删除' : '删除影响范围'} onClose={onClose}><p className={s.modalIntro}>“{preview.title}”将实际删除。归档可以保留历史。</p><ul className={s.deleteSummary}><li>删除 {preview.plans} 条计划</li><li>删除 {preview.progress} 条独立进展、{preview.completions} 条完成记录及其历史快照（含撤销记录）</li><li>保留并提升 {preview.children.length} 个直接子任务：{preview.children.map(c => c.title).join('、') || '无'}</li><li>保留 {preview.lifeEvents} 条独立生活事件及图片，只解除此任务关联</li><li>其他目标、任务及子任务的历史均保留</li></ul>{second && <label className={s.checkLabel}><input type="checkbox" checked={confirmed} aria-label="确认删除历史" onChange={e => setConfirmed(e.target.checked)} />我确认删除上述历史，无法撤销</label>}<div className={s.exitActions}><button className={s.dangerButton} disabled={busy || (second && !confirmed)} onClick={() => { if (preview.hasHistory && !second) setSecond(true); else onDelete(second && confirmed); }}>{busy ? '正在删除…' : preview.hasHistory && !second ? '查看完毕，继续确认' : '彻底删除'}</button><button className={s.secondaryButton} disabled={busy} onClick={onClose}>保留任务</button></div></Modal>;
}
