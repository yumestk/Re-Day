import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { type RunMutation, type Workspace, type QuestDraft } from './api';
import { Icon } from './ui';
import s from './App.module.css';

export type SaveRef = React.RefObject<(() => Promise<boolean>) | null>;
export interface EditorProps { data: Workspace; run: RunMutation; busy: boolean; onChanged: () => Promise<void>; onClose: () => void; onDirty: (dirty: boolean) => void; saveRef: SaveRef }

export function EditorFrame({ readOnly = false, title, children, after, dirty, busy, onSave, onClose, onDirty, saveRef, submitLabel = '保存修改' }: { submitLabel?: string; readOnly?: boolean; title: string; children: ReactNode; after?: ReactNode; dirty: boolean; busy: boolean; onSave: () => Promise<boolean>; onClose: () => void; onDirty: (v: boolean) => void; saveRef: SaveRef }) {
  const form = useRef<HTMLFormElement>(null);
  const [discard, setDiscard] = useState(false);
  const close = useCallback(() => { if (dirty) setDiscard(true); else onClose(); }, [dirty, onClose]);
  useEffect(() => { onDirty(dirty); return () => onDirty(false); }, [dirty, onDirty]);
  async function save() { if (readOnly || busy || !form.current?.reportValidity()) return false; return onSave(); }
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
      <fieldset className={s.formBody} disabled={busy || readOnly}>{children}</fieldset>
      {discard && <div className={s.unsavedPrompt}><strong>修改尚未保存</strong><p>保存修改，或放弃这次编辑。</p><button type="button" className={s.secondaryButton} onClick={onClose}>放弃修改</button><button type="button" className={s.textButton} onClick={() => setDiscard(false)}>继续编辑</button></div>}
      <div className={s.editorActions}><span>{busy ? '正在保存…' : dirty ? '有未保存的修改' : '已保存'}</span><button type="submit" className={s.primaryButton} disabled={busy || readOnly}>{submitLabel}<Icon name="arrow" size={16} /></button></div>
    </form>{after}
  </aside>;
}

export function QuestLinks({ data, questId, draft, onChange, allowParent = true }: { allowParent?: boolean; data: Workspace; questId?: string; draft: QuestDraft; onChange: (draft: QuestDraft) => void }) {
  const descendants = new Set<string>(questId ? [questId] : []);
  for (let i = 0; i < data.quests.length; i++) for (const q of data.quests) if (q.parentId && descendants.has(q.parentId)) descendants.add(q.id);
  return <>
    <label>父任务<select aria-label="父任务" disabled={!allowParent} value={draft.parentId ?? ''} onChange={e => onChange({ ...draft, parentId: e.target.value || null })}><option value="">独立任务（顶层）</option>{data.quests.filter(q => !descendants.has(q.id) && q.mode === 'single').map(q => <option key={q.id} value={q.id}>{q.title}{q.archivedOn ? ' · 已归档' : ''}</option>)}</select></label>
    <fieldset className={s.goalChoices}><legend>关联目标 <span>可多选</span></legend>{data.goals.map(g => <label key={g.id}><input type="checkbox" aria-label={`关联目标：${g.name}`} checked={draft.goalIds.includes(g.id)} onChange={e => onChange({ ...draft, goalIds: e.target.checked ? [...draft.goalIds, g.id] : draft.goalIds.filter(id => id !== g.id) })} /><span>{g.name}<small>{g.kind === 'main' ? '主线' : '支线'}{g.archived ? ' · 已归档' : ''}</small></span></label>)}{!data.goals.length && <p className={s.formHint}>可以先创建任务，之后再到 Quests 添加目标。</p>}</fieldset>
  </>;
}
