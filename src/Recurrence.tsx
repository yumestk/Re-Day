import { useCallback, useEffect, useRef, useState } from 'react';
import { api, errorMessage, slots, type Quest, type RuleDraft, type RulePreview, type RuleRequest, type Slot, type TaskItem } from './api';
import { EditorFrame, QuestLinks, type EditorProps } from './editors';
import { Icon, Modal } from './ui';
import s from './App.module.css';

export function RuleSummary({ quest, today }: { quest: Quest; today: string }) {
  const scheduled = quest.recurrence.filter(v => v.enabled && (v.effectiveUntil === null || v.effectiveUntil > today) && (!v.endOn || v.endOn >= today)).at(-1);
  if (quest.mode === 'single') return null;
  return <div className={s.ruleSummary}><span>RECURRING / 重复任务</span><b>{scheduled ? scheduled.frequency === 'daily' ? '每日' : scheduled.frequency === 'weekly' ? `每周 ${['一', '二', '三', '四', '五', '六', '日'].filter((_, i) => (scheduled.weekdays ?? 0) & (1 << i)).map(v => `周${v}`).join('、')}` : `每隔 ${scheduled.intervalDays} 天` : '规则已停用或结束'}</b><small>{scheduled ? `${scheduled.effectiveFrom} 起 · ${slots.find(v => v.id === scheduled.slot)?.label}${scheduled.startTime ? ` ${scheduled.startTime}` : ''}${scheduled.endOn ? ` · 至 ${scheduled.endOn}（含）` : ''}${scheduled.effectiveUntil ? ` · ${scheduled.effectiveUntil} 起此版本停止` : ''}` : '恢复归档或取消状态不会自动重新启用规则。'}<br />每次独立完成；漏做不会自动累积。</small></div>;
}

export function RecurrenceEditor({ quest, initialDate, ...props }: EditorProps & { quest: Quest; initialDate?: string }) {
  const from = initialDate && initialDate >= props.data.today ? initialDate : props.data.today;
  const activeVersion = quest.recurrence.filter(v => v.effectiveFrom <= from && (!v.effectiveUntil || v.effectiveUntil > from)).at(-1);
  const version = activeVersion ?? quest.recurrence.filter(v => !v.effectiveUntil || v.effectiveUntil > v.effectiveFrom).at(-1) ?? quest.recurrence.at(-1);
  const [draft, setDraft] = useState<RuleDraft>(() => ({ title: activeVersion?.snapshot.title ?? quest.title, description: activeVersion?.snapshot.description ?? quest.description, goalIds: activeVersion?.snapshot.goals.map(g => g.id) ?? quest.goalIds, parentId: quest.parentId, date: from, slot: version?.slot ?? 'morning', startTime: version?.startTime ?? null, endTime: version?.endTime ?? null, frequency: version?.frequency ?? 'daily', intervalDays: version?.intervalDays ?? null, weekdays: version?.weekdays ?? null, endOn: version?.endOn && version.endOn >= from ? version.endOn : null, enabled: version ? version.enabled && (!version.effectiveUntil || version.effectiveUntil > from) && (!version.endOn || version.endOn >= from) : true }));
  const original = useRef(JSON.stringify(draft));
  const base = useRef({ revision: quest.revision, versionId: crypto.randomUUID() });
  const [preview, setPreview] = useState<{ result: RulePreview; request: RuleRequest } | null>(null);
  const closePreview = useCallback(() => setPreview(null), []);
  const save = async () => {
    const request = { ...draft, datasetId: props.data.datasetId, questId: quest.id, versionId: base.current.versionId, expectedRevision: base.current.revision };
    const result = await props.run(quest.id, () => api.previewRule(request));
    if (result) setPreview({ result, request });
    return false;
  };
  const commit = async () => {
    if (!preview) return;
    const result = await props.run(quest.id, () => api.saveRule(preview.request, preview.result.fingerprint));
    setPreview(null);
    if (result === null) return;
    await props.onChanged(); props.onClose();
  };
  const changeFrequency = (frequency: RuleDraft['frequency']) => setDraft({ ...draft, frequency, weekdays: frequency === 'weekly' ? 1 : null, intervalDays: frequency === 'interval' ? 2 : null });
  return <><EditorFrame {...props} title={quest.mode === 'single' ? '设置重复规则' : '本次及未来'} submitLabel="预览影响" dirty={JSON.stringify(draft) !== original.current} onSave={save}>
    <p className={s.formHint}>保留历史、已有执行与补做；未来已设置的单次覆盖会保留，并在预览中列明。停用优先停止正常安排。</p>
    <label>任务标题<input autoFocus required aria-label="重复任务标题" value={draft.title} onChange={e => setDraft({ ...draft, title: e.target.value })} /></label>
    <label>补充说明<textarea aria-label="重复任务说明" rows={3} value={draft.description} onChange={e => setDraft({ ...draft, description: e.target.value })} /></label>
    <QuestLinks data={props.data} questId={quest.id} draft={draft} onChange={value => setDraft({ ...draft, ...value })} />
    <label>生效日期<input aria-label="重复生效日期" type="date" required min={props.data.today} value={draft.date} onChange={e => setDraft({ ...draft, date: e.target.value })} /></label>
    <label className={s.checkLabel}><input aria-label="启用重复规则" type="checkbox" checked={draft.enabled} onChange={e => setDraft({ ...draft, enabled: e.target.checked })} />从生效日期起启用规则</label>
    {!draft.enabled && <p className={s.fieldError}>从所选日期起停止正常重复安排，已有补做和执行历史保留。</p>}
    <label>重复频率<select aria-label="重复频率" value={draft.frequency} onChange={e => changeFrequency(e.target.value as RuleDraft['frequency'])}><option value="daily">每日</option><option value="weekly">每周选定星期</option><option value="interval">每隔 N 天</option></select></label>
    {draft.frequency === 'weekly' && <fieldset className={s.weekdays}><legend>选择星期</legend>{['一', '二', '三', '四', '五', '六', '日'].map((day, i) => <label key={day}><input aria-label={`周${day}`} type="checkbox" checked={!!((draft.weekdays ?? 0) & (1 << i))} onChange={e => setDraft({ ...draft, weekdays: e.target.checked ? (draft.weekdays ?? 0) | (1 << i) : (draft.weekdays ?? 0) & ~(1 << i) })} /><span>{day}</span></label>)}</fieldset>}
    {draft.frequency === 'interval' && <label>间隔天数<input aria-label="间隔天数" type="number" required min={1} step={1} value={draft.intervalDays ?? ''} onChange={e => setDraft({ ...draft, intervalDays: e.target.value ? Number(e.target.value) : null })} /><small className={s.formHint}>以本次生效日期为锚点，按日历日计算。</small></label>}
    <label>截止日期<span className={s.optional}>可选，包含当日</span><input aria-label="重复截止日期" type="date" min={draft.date} value={draft.endOn ?? ''} onChange={e => setDraft({ ...draft, endOn: e.target.value || null })} /></label>
    <label>默认时段<select aria-label="重复默认时段" value={draft.slot} onChange={e => setDraft({ ...draft, slot: e.target.value as Slot })}>{slots.map(v => <option key={v.id} value={v.id}>{v.label}</option>)}</select></label>
    <div className={s.timeFields}><label>开始<input aria-label="重复开始时间" type="time" value={draft.startTime ?? ''} onChange={e => setDraft({ ...draft, startTime: e.target.value || null })} /></label><label>结束<input aria-label="重复结束时间" type="time" value={draft.endTime ?? ''} onChange={e => setDraft({ ...draft, endTime: e.target.value || null })} /></label></div>
    <p className={s.formHint}>有子任务的任务不能重复；已完成任务须先明确撤销完成。普通任务转换时，今天及未来无执行的旧普通安排会停止并保留原行。</p>
  </EditorFrame>{preview && <Modal title="确认规则影响" onClose={closePreview}>
    <p className={s.modalIntro}>{preview.request.date} 起{preview.request.enabled ? '应用新规则，正常实例在查看对应日期时生成。' : '停用正常重复安排。'}原执行与补做不改写。</p>
    <div className={s.rulePreview}><h3>已生成安排的变化</h3>{preview.result.changes.map((v, i) => <p key={i}><b>{v.date} · {v.title}</b><br />{v.action}</p>)}{!preview.result.changes.length && <p>没有需要改写的已生成安排。</p>}<h3>保留的单次覆盖</h3><p>{preview.result.retainedOverrides.join('、') || '无'}</p><p>另有 {preview.result.preservedFacts} 条带执行事实的安排保持原内容。未查看的日期按有效版本生成，不预生成无限未来。</p></div>
    <div className={s.exitActions}><button className={s.primaryButton} disabled={props.busy} onClick={() => void commit()}>确认应用规则</button><button className={s.secondaryButton} disabled={props.busy} onClick={closePreview}>返回修改</button></div>
  </Modal>}</>;
}

export function InstancePicker({ quest, initialDate, onSelect, ...props }: EditorProps & { quest: Quest; initialDate?: string; onSelect: (item: TaskItem) => void }) {
  const [date, setDate] = useState(initialDate ?? props.data.today);
  const [items, setItems] = useState<TaskItem[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => { props.onDirty(false); props.saveRef.current = null; }, [props.onDirty, props.saveRef]);
  useEffect(() => {
    let cancelled = false; setLoading(true); setError(''); setItems([]);
    void api.instances(props.data.datasetId, quest.id, date).then(items => { if (!cancelled) setItems(items); }).catch(error => { if (!cancelled) setError(errorMessage(error)); }).finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [quest.id, props.data.datasetId, date, attempt]);
  useEffect(() => { const key = (e: KeyboardEvent) => { if (e.key === 'Escape') props.onClose(); }; window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key); }, [props.onClose]);
  return <aside className={s.editor} aria-label="选择重复实例"><div className={s.editorHeader}><span className={s.eyebrow}>ONE OCCURRENCE AT A TIME</span><button className={s.iconButton} aria-label="关闭任务详情" onClick={props.onClose}><Icon name="close" /></button></div><div className={s.editorTitle}><h2>选择已有实例<small>每次独立记录进展和完成。</small></h2></div><RuleSummary quest={quest} today={props.data.today} /><label className={s.instanceDate}>实例日期<input type="date" required aria-label="实例日期" value={date} onChange={e => { if (e.target.value) setDate(e.target.value); }} /></label>
    {error && <p className={s.fieldError} role="alert">{error}<button onClick={() => setAttempt(attempt + 1)}>重新读取</button></p>}
    {loading ? <p className={s.formHint}>正在读取正常实例与已有补做…</p> : items.length ? <div className={s.instanceChoices}>{items.map(item => <button key={item.planId} data-instance-id={item.planId} data-origin={item.origin} onClick={() => onSelect(item)}><span>{item.origin === 'makeup' ? `补做 · 来自 ${item.sourceOn ?? item.occurrenceOn}` : '正常重复实例'}{item.isOverride ? ' · 单次覆盖' : ''}</span><b>{item.title}</b><small>{item.scheduledOn} · {item.completed ? '已完成' : item.disposition === 'rescheduled' ? '已顺延' : '未完成'}</small></button>)}</div> : <p className={s.collectionEmpty}>该日没有正常实例或已有补做安排。不会手动增加额外重复实例；可选择其他日期。</p>}
  </aside>;
}

export function RescheduleEditor({ quest, plan, ...props }: EditorProps & { quest: Quest; plan: TaskItem }) {
  const tomorrow = new Date(`${plan.scheduledOn > props.data.today ? plan.scheduledOn : props.data.today}T12:00:00`); tomorrow.setDate(tomorrow.getDate() + 1);
  const value = `${tomorrow.getFullYear()}-${String(tomorrow.getMonth() + 1).padStart(2, '0')}-${String(tomorrow.getDate()).padStart(2, '0')}`;
  const [draft, setDraft] = useState({ date: value, slot: plan.slot, startTime: plan.startTime ?? '', endTime: plan.endTime ?? '' });
  const original = useRef(JSON.stringify(draft));
  const intent = useRef({ id: crypto.randomUUID(), plan });
  const save = async () => {
    const result = await props.run(plan.planId, () => api.reschedule(props.data.datasetId, intent.current.plan, intent.current.id, { ...draft, startTime: draft.startTime || null, endTime: draft.endTime || null }));
    if (!result) return false; await props.onChanged(); return true;
  };
  return <EditorFrame {...props} title="顺延或重新安排本次" submitLabel="确认生成补做" dirty={original.current !== JSON.stringify(draft)} onSave={save}>
    <p className={s.executionTitle}>{plan.title}</p><p className={s.formHint}>来自 {plan.scheduledOn} 的安排。原计划与进展保留；目标日期即使已有正常实例，也会分别展示、独立完成。</p><label>补做日期<input required aria-label="补做日期" type="date" min={props.data.today} value={draft.date} onChange={e => setDraft({ ...draft, date: e.target.value })} /></label><label>时间段<select aria-label="补做时段" value={draft.slot} onChange={e => setDraft({ ...draft, slot: e.target.value as Slot })}>{slots.map(v => <option key={v.id} value={v.id}>{v.label}</option>)}</select></label><div className={s.timeFields}><label>开始<input aria-label="补做开始时间" type="time" value={draft.startTime} onChange={e => setDraft({ ...draft, startTime: e.target.value })} /></label><label>结束<input aria-label="补做结束时间" type="time" value={draft.endTime} onChange={e => setDraft({ ...draft, endTime: e.target.value })} /></label></div>
  </EditorFrame>;
}
