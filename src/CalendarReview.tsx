import { useEffect, useState } from 'react';
import { api, errorMessage, slots, type DateView, type MonthView, type PlanReview, type TaskItem, type Workspace } from './api';
import { RecordList } from './Quests';
import { Icon } from './ui';
import { type EditorProps } from './editors';
import s from './App.module.css';

const resultLabel = { completed: '当日完成', progressed: '当日有进展', unexecuted: '当日未执行', planned: '待执行' };
const dispositionLabel = { active: '安排保留', skipped: '已放弃本次', rescheduled: '已重新安排', withdrawn: '安排已停止' };
export function CalendarReview({ mode, date, refresh, workspace, busy, onDate, onTask, onExecute, onReschedule, onSkip, onCreate, onSchedule, onReviewed, onTomorrow }: {
  mode: 'calendar' | 'review'; date: string; refresh: number; workspace: Workspace; busy: boolean;
  onDate: (date: string) => void; onTask: (item: TaskItem) => void; onExecute: (item: TaskItem, kind: 'progress' | 'completion') => void;
  onReschedule: (item: TaskItem) => void; onSkip: (item: TaskItem) => void; onCreate: () => void; onSchedule: () => void;
  onReviewed: (date: string) => Promise<void>; onTomorrow: () => void;
}) {
  const [loadedView, setView] = useState<DateView | null>(null);
  const view = loadedView?.date === date ? loadedView : null;
  const [monthView, setMonthView] = useState<MonthView | null>(null);
  const [dateError, setDateError] = useState('');
  const [monthError, setMonthError] = useState('');
  const error = dateError || (mode === 'calendar' ? monthError : '');
  const [retry, setRetry] = useState(0);
  const month = date.slice(0, 7);
  useEffect(() => { let cancelled = false; setView(null); setDateError(''); void api.loadDate(date).then(result => { if (!cancelled) setView(result); }).catch(e => { if (!cancelled) setDateError(errorMessage(e)); }); return () => { cancelled = true; }; }, [date, refresh, retry]);
  useEffect(() => { if (mode !== 'calendar') return; let cancelled = false; setMonthView(null); setMonthError(''); void api.loadCalendar(month).then(result => { if (!cancelled) setMonthView(result); }).catch(e => { if (!cancelled) setMonthError(errorMessage(e)); }); return () => { cancelled = true; }; }, [month, mode, refresh, retry]);
  const changeMonth = (delta: number) => { const value = new Date(`${month}-01T12:00:00`); value.setMonth(value.getMonth() + delta); onDate(`${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, '0')}-01`); };
  const offset = (new Date(`${month}-01T12:00:00`).getDay() + 6) % 7;
  return <section className={s.calendarPage} data-loaded-date={view?.date} aria-label={mode === 'calendar' ? '日历与日期详情' : '每日回顾'}>
    <header className={s.collectionHero}><span className={s.eyebrow}>{mode === 'calendar' ? 'YOUR DAYS / YOUR STORY' : 'PAUSE / LOOK BACK / NEXT STEP'}</span><h1>{mode === 'calendar' ? 'CALENDAR' : 'REVIEW'}<span>.</span></h1><p>{mode === 'calendar' ? '计划留下轨迹，事实归于发生的那一天。' : '看见今天的推进，给明天留一点位置。'}</p></header>
    {error && <div className={s.error} role="alert">{error}<button onClick={() => setRetry(retry + 1)}>重新读取</button></div>}
    {mode === 'calendar' && <section className={s.month} aria-label="月历"><div className={s.monthHeading}><button className={s.iconButton} aria-label="上个月" onClick={() => changeMonth(-1)}>‹</button><h2>{month.replace('-', ' / ')}</h2><button className={s.iconButton} aria-label="下个月" onClick={() => changeMonth(1)}>›</button><button className={s.todayButton} onClick={() => onDate(workspace.today)}>今天</button></div><div className={s.monthGrid}>{['一','二','三','四','五','六','日'].map(v => <span className={s.weekday} key={v}>周{v}</span>)}{Array.from({ length: offset }, (_, i) => <span key={`empty-${i}`} />)}{(monthView?.month === month ? monthView.days : undefined)?.map(day => <button key={day.date} data-calendar-date={day.date} aria-pressed={day.date === date} className={`${s.monthDay} ${day.date === date ? s.monthSelected : ''} ${day.date === workspace.today ? s.monthToday : ''}`} onClick={() => onDate(day.date)}><b>{Number(day.date.slice(8))}<small>{day.reviewed ? '已回顾' : ''}</small></b><span>{day.plans ? `${day.plans} 安排` : '自由的一天'}</span>{!!day.completed && <em>{day.completed} 完成</em>}{!!day.progressed && <em>{day.progressed} 有进展</em>}{!!day.lifeEvents && <em>{day.lifeEvents} 生活事件</em>}</button>)}</div>{!monthView && <p role="status">正在读取月历…</p>}<p className={s.formHint}>完成 / 有进展按实际发生日期统计，同一任务的多条进展合为一项；计划处置不等于执行事实。</p></section>}
    <div className={s.collectionHeading}><h2>{mode === 'review' ? '回顾这一天' : '日期详情'}</h2><input type="date" aria-label={mode === 'review' ? '回顾日期' : '日历日期'} value={date} max={mode === 'review' ? workspace.today : undefined} onChange={e => { if (e.target.value) onDate(e.target.value); }} /></div>
    {!view ? <p role="status">正在读取这一天的事实…</p> : <>
      <div className={s.dayTotals} aria-label="当日结果统计"><div><b>{view.summary.completed}</b><span>当日完成</span></div><div><b>{view.summary.progressed}</b><span>仅有进展</span></div><div><b>{view.summary.unexecuted}</b><span>当日未执行的安排</span></div><div><b>{view.review.reviewedAt ? '✓' : '—'}</b><span>{view.review.reviewedAt ? '已主动回顾' : '尚未标记回顾'}</span></div></div>
      {(['completed', 'progressed'] as const).map(result => <section className={s.reviewSection} aria-label={resultLabel[result]} key={result}><h3>{result === 'completed' ? '已完成的事实' : '当天有进展，仍未完成'}</h3>{view.activities.filter(a => a.result === result).map(group => <div key={group.key} data-activity-key={group.key} data-result={group.result}><RecordList records={group.records} /></div>)}{!view.activities.some(a => a.result === result) && <p className={s.formHint}>这一天没有此类记录。</p>}</section>)}
      <section className={s.reviewSection} aria-label="计划与后续处理"><div className={s.collectionHeading}><h3>计划与后续处理</h3><div className={s.contextActions}><button onClick={onSchedule}>安排已有任务</button><button onClick={onCreate}>＋ 新建并安排</button></div></div>{view.plans.map(plan => <PlanRow key={plan.item.planId} plan={plan} busy={busy} onTask={onTask} onExecute={onExecute} onReschedule={onReschedule} onSkip={onSkip} />)}{!view.plans.length && <p className={s.collectionEmpty}>没有预先安排，也可以记录真实推进。</p>}</section>
      {!!view.events.length && <section className={s.reviewSection} aria-label="当日生活事件"><h3>生活足迹</h3>{view.events.map(event => <article className={s.eventSummary} key={event.id}><b>{event.isMilestone ? '◇ 里程碑 · ' : ''}{event.title}</b><p>{event.note}</p></article>)}</section>}
      {view.records.some(r => r.revokedAt) && <details className={s.planHistory}><summary>已撤销的完成记录</summary><RecordList records={view.records.filter(r => r.revokedAt)} /></details>}
      <div className={s.reviewFinish}><div><h3>{view.review.reviewedAt ? '这一天已回顾' : '按自己的节奏收尾'}</h3><p>可以留下未处理项。离开页面不会自动标记完成回顾。</p></div><div className={s.contextActions}>{date <= view.today && <button className={s.primaryButton} disabled={busy || !!view.review.reviewedAt} onClick={() => void onReviewed(date)}>{view.review.reviewedAt ? '已完成回顾' : '完成回顾'}</button>}<button className={s.secondaryButton} onClick={onTomorrow}>规划明天</button></div></div>
    </>}
  </section>;
}
function PlanRow({ plan, busy, onTask, onExecute, onReschedule, onSkip }: { plan: PlanReview; busy: boolean; onTask: (item: TaskItem) => void; onExecute: (item: TaskItem, kind: 'progress' | 'completion') => void; onReschedule: (item: TaskItem) => void; onSkip: (item: TaskItem) => void }) {
  const { item } = plan;
  return <article className={s.reviewPlan} data-review-plan={item.planId} data-plan-title={item.title} data-result={plan.result} data-disposition={item.disposition}><div><button className={s.planTitle} onClick={() => onTask(item)}>{item.title}<Icon name="chevron" size={15} /></button><p>{slots.find(s => s.id === item.slot)?.label} · {item.startTime ?? '时间自由'}{item.endTime ? `–${item.endTime}` : ''} · {item.origin === 'makeup' ? `补做，来自 ${item.sourceOn ?? item.occurrenceOn}` : item.origin === 'recurring' ? '正常重复实例' : '普通安排'} · {item.goals.map(g => g.name).join(' / ') || '无目标归属'}</p><span className={s.resultPill}>{resultLabel[plan.result]}</span><small>{dispositionLabel[item.disposition]}{plan.destinationOn ? ` → ${plan.destinationOn}` : ''}{item.completedOn && item.completedOn !== item.scheduledOn ? ` · 任务 / 实例于 ${item.completedOn} 完成，不计入本日完成` : ''}{item.completed && plan.result !== 'completed' ? ' · 当前已完成，无需再次处理' : ''}</small></div>{plan.canManage && <div className={s.contextActions}><button disabled={busy} onClick={() => onExecute(item,'progress')}>记录进展</button><button disabled={busy} onClick={() => onExecute(item,'completion')}>记录完成</button><button disabled={busy} onClick={() => onReschedule(item)}>顺延到明天</button><button disabled={busy} onClick={() => onReschedule(item)}>重新安排日期</button><button disabled={busy} onClick={() => onSkip(item)}>放弃本次</button></div>}</article>;
}
export function SchedulePicker({ data, onSelect, ...props }: EditorProps & { onSelect: (id: string) => void }) {
  const [search, setSearch] = useState('');
  useEffect(() => { props.onDirty(false); props.saveRef.current = null; }, [props.onDirty, props.saveRef]);
  useEffect(() => { const key = (e: KeyboardEvent) => { if (e.key === 'Escape') props.onClose(); }; window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key); }, [props.onClose]);
  const tasks=data.quests.filter(q => q.mode==='single' && q.status==='open' && !q.archivedOn && q.title.includes(search));
  return <aside className={s.editor} aria-label="安排已有任务"><div className={s.editorHeader}><span className={s.eyebrow}>CHOOSE YOUR NEXT STEP</span><button aria-label="关闭任务详情" className={s.iconButton} onClick={props.onClose}><Icon name="close" /></button></div><div className={s.editorTitle}><h2>安排已有任务</h2></div><label className={s.instanceDate}>搜索任务<input aria-label="安排任务搜索" value={search} onChange={e=>setSearch(e.target.value)} /></label><div className={s.instanceChoices}>{tasks.map(q=><button key={q.id} data-schedule-quest={q.id} onClick={()=>onSelect(q.id)}>{q.title}</button>)}</div>{!tasks.length&&<p className={s.formHint}>没有匹配的活跃普通任务。</p>}<p className={s.instanceDate}>重复任务按规则自动出现；额外补做需明确顺延已有实例。</p></aside>;
}
