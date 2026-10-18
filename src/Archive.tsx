import { useCallback, useEffect, useRef, useState } from 'react';
import { api, errorMessage, type LifeEvent, type EventDraft, type ImageView, type Workspace } from './api';
import { EditorFrame, type EditorProps } from './editors';
import { Icon, Modal } from './ui';
import s from './App.module.css';

export function Photo({ image }: { image: ImageView }) {
  const [failed, setFailed] = useState(false);
  const [opened, setOpened] = useState(false);
  const close = useCallback(() => setOpened(false), []);
  return failed ? <span className={s.imageError} role="alert">图片无法读取，请检查本地资源。</span> : <><button type="button" className={s.photoButton} aria-label="查看生活记录图片" onClick={() => setOpened(true)}><img src={image.url} alt="生活记录图片" loading="lazy" onError={() => setFailed(true)} /></button>{opened && <Modal title="生活记录图片" onClose={close}><img className={s.fullPhoto} src={image.url} alt="生活记录图片预览" /></Modal>}</>;
}
export function Archive({ data, refresh, onCreate, onEdit }: { data: Workspace; refresh: number; onCreate: () => void; onEdit: (id: string) => void }) {
  const [events, setEvents] = useState<LifeEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const [milestones, setMilestones] = useState(false);
  const [search, setSearch] = useState('');
  useEffect(()=>{let cancelled=false;setLoading(true);setError('');void api.events(data.datasetId).then(result=>{if(!cancelled)setEvents(result);}).catch(e=>{if(!cancelled)setError(errorMessage(e));}).finally(()=>{if(!cancelled)setLoading(false);});return()=>{cancelled=true;};},[data.datasetId,refresh,retry]);
  const visible=events.filter(e=>(!milestones||e.isMilestone)&&[e.title,e.note,...e.tags].some(v=>v.includes(search)));
  return <section className={s.archivePage} aria-label="生活记录"><header className={s.collectionHero}><span className={s.eyebrow}>REAL MOMENTS / WORTH KEEPING</span><h1>ARCHIVE<span>.</span></h1><p>留下经历，也留下你主动确认的里程碑。</p></header><div className={s.collectionHeading}><h2>生活足迹</h2><button className={s.primaryButton} onClick={onCreate}>＋ 生活事件</button></div><div className={s.questFilters}><input aria-label="搜索生活记录" value={search} placeholder="标题、标签或备注…" onChange={e=>setSearch(e.target.value)} /><label className={s.checkLabel}><input type="checkbox" aria-label="仅里程碑" checked={milestones} onChange={e=>setMilestones(e.target.checked)} />仅里程碑</label></div>{error&&<div className={s.error} role="alert">{error}<button onClick={()=>setRetry(v=>v+1)}>重新读取</button></div>}{loading?<p role="status">正在读取生活记录…</p>:visible.map(event=><article className={s.eventCard} data-event-title={event.title} key={event.id}><div className={s.eventCardHeading}><time>{event.eventOn}</time><button className={s.planTitle} onClick={()=>onEdit(event.id)}><h3>{event.isMilestone?'◇ ':''}{event.title}</h3><Icon name="chevron" size={17}/></button><button className={s.textButton} aria-label={`编辑生活事件：${event.title}`} onClick={()=>onEdit(event.id)}>编辑</button></div>{event.note&&<p className={s.eventNote}>{event.note}</p>}<div className={s.eventTags}>{event.tags.map(tag=><span key={tag}>#{tag}</span>)}{event.isMilestone&&<b>里程碑</b>}</div><div className={s.photoGrid}>{event.images.map(image=><Photo key={image.id} image={image}/>)}</div><small>{event.goalId?`目标 · ${data.goals.find(g=>g.id===event.goalId)?.name??'已解除关联'}`:''}{event.questId?`　任务 · ${data.quests.find(q=>q.id===event.questId)?.title??'已解除关联'}`:''}</small></article>)}{!loading&&!visible.length&&!error&&<p className={s.collectionEmpty}>还没有匹配的记录。可以从一件真实发生的小事开始。</p>}</section>;
}
export function EventEditor({ eventId, initialQuestId, initialGoalId, initialMilestone, initialDate, onCleanupPending, ...props }: EditorProps & { onCleanupPending: () => void; eventId?: string; initialQuestId?: string; initialGoalId?: string; initialMilestone?: boolean; initialDate?: string }) {
  const [event,setEvent]=useState<LifeEvent|null>(null);
  const [loaded,setLoaded]=useState(!eventId);
  const [error,setError]=useState('');
  const [retry,setRetry]=useState(0);
  useEffect(()=>{if(!eventId)return;let cancelled=false;setLoaded(false);void api.event(props.data.datasetId,eventId).then(e=>{if(!cancelled){setEvent(e);setError('');setLoaded(true);}}).catch(e=>{if(!cancelled)setError(errorMessage(e));});return()=>{cancelled=true;};},[props.data.datasetId,eventId,retry]);
  if(!loaded)return <aside className={s.editor} aria-label="读取生活事件"><div className={s.editorHeader}><button className={s.iconButton} aria-label="关闭任务详情" onClick={props.onClose}><Icon name="close"/></button></div>{error?<p role="alert">{error}<button onClick={()=>setRetry(v=>v+1)}>重新读取</button></p>:<p>正在读取生活事件…</p>}</aside>;
  return <EventForm key={event?.id??'new-event'} {...props} event={event} initialQuestId={initialQuestId} initialGoalId={initialGoalId} initialMilestone={initialMilestone} initialDate={initialDate} onCleanupPending={onCleanupPending}/>;
}
function EventForm({event, initialQuestId, initialGoalId, initialMilestone, initialDate, onCleanupPending, ...props}:EditorProps & {onCleanupPending: () => void; event:LifeEvent|null;initialQuestId?:string;initialGoalId?:string;initialMilestone?:boolean;initialDate?:string}) {
  const [draft,setDraft]=useState<EventDraft>(()=>({title:event?.title??props.data.quests.find(q=>q.id===initialQuestId)?.title??props.data.goals.find(g=>g.id===initialGoalId)?.name??'',eventOn:event?.eventOn??initialDate??props.data.today,note:event?.note??'',tags:event?.tags??[],isMilestone:event?.isMilestone??!!initialMilestone,goalId:event?.goalId??initialGoalId??null,questId:event?.questId??initialQuestId??null,imageIds:event?.images.map(i=>i.id)??[]}));
  const [tags,setTags]=useState(draft.tags.join('，'));
  const [images,setImages]=useState<ImageView[]>(event?.images??[]);
  const [deletion,setDeletion]=useState(false);
  const staged=useRef<string[]>([]);
  const id=useRef(event?.id??crypto.randomUUID());
  const original=useRef(JSON.stringify(draft));
  const tagValue=Array.from(new Set(tags.split(/[,，]/).map(s=>s.trim()).filter(Boolean)));
  const value={...draft,tags:tagValue};
  const closeDelete=useCallback(()=>setDeletion(false),[]);
  useEffect(()=>()=>{void api.discardImages(props.data.datasetId,staged.current).catch(()=>{});},[props.data.datasetId]);
  const pick=async()=>{const picked=await props.run('event-images',()=>api.pickImages(props.data.datasetId));if(!picked)return;staged.current.push(...picked.filter(i=>i.staged).map(i=>i.id));setImages(current=>[...current,...picked.filter(p=>!current.some(i=>i.id===p.id))]);setDraft(d=>({...d,imageIds:[...d.imageIds,...picked.filter(p=>!d.imageIds.includes(p.id)).map(i=>i.id)]}));};
  const save=async()=>{const result=await props.run(id.current,()=>api.saveEvent(props.data.datasetId,id.current,event?.revision??null,value));if(!result)return false;await props.onChanged();if(result.cleanupPending)onCleanupPending();return true;};
  const remove=async()=>{if(!event)return;const result=await props.run(id.current,()=>api.deleteEvent(props.data.datasetId,event));if(!result)return;await props.onChanged();if(result.cleanupPending)onCleanupPending();props.onClose();};
  const moveImage=(index:number)=>setDraft(d=>{const imageIds=[...d.imageIds];[imageIds[index-1],imageIds[index]]=[imageIds[index],imageIds[index-1]];return {...d,imageIds};});
  return <><EditorFrame {...props} title={event?'生活事件详情':'新的生活事件'} dirty={original.current!==JSON.stringify(value)} onSave={save} submitLabel="保存生活记录" after={event&&<div className={s.questContext}><small>录入于 {new Date(event.createdAt).toLocaleString('zh-CN')}</small><button className={s.dangerButton} disabled={props.busy} onClick={()=>setDeletion(true)}>删除生活事件</button></div>}>
    <label>事件标题<input autoFocus required maxLength={200} aria-label="事件标题" value={draft.title} onChange={e=>setDraft({...draft,title:e.target.value})}/></label><label>发生日期<input type="date" required aria-label="事件日期" value={draft.eventOn} onChange={e=>setDraft({...draft,eventOn:e.target.value})}/></label><label>简短备注<textarea rows={4} maxLength={4000} aria-label="事件备注" value={draft.note} onChange={e=>setDraft({...draft,note:e.target.value})}/></label><label>标签<span className={s.optional}>用逗号分隔</span><input aria-label="事件标签" value={tags} onChange={e=>setTags(e.target.value)}/></label><label className={s.checkLabel}><input type="checkbox" aria-label="标记为里程碑" checked={draft.isMilestone} onChange={e=>setDraft({...draft,isMilestone:e.target.checked})}/>将这条生活事件标记为里程碑</label><p className={s.formHint}>由你主动确认；取消标记会保留事件本身。</p>
    <label>关联目标<span className={s.optional}>可不关联</span><select aria-label="事件关联目标" value={draft.goalId??''} onChange={e=>setDraft({...draft,goalId:e.target.value||null})}><option value="">无目标关联</option>{props.data.goals.map(g=><option key={g.id} value={g.id}>{g.name}{g.status==='completed'?' · 已完成':''}{g.archived?' · 已归档':''}</option>)}</select></label><label>关联任务<span className={s.optional}>可不关联</span><select aria-label="事件关联任务" value={draft.questId??''} onChange={e=>setDraft({...draft,questId:e.target.value||null})}><option value="">无任务关联</option>{props.data.quests.map(q=><option key={q.id} value={q.id}>{q.title}{q.status==='completed'?' · 已完成':''}{q.archivedOn?' · 已归档':''}</option>)}</select></label>
    <div className={s.contextHeading}><h3>图片 · {draft.imageIds.length}</h3><button type="button" className={s.secondaryButton} disabled={props.busy} onClick={()=>void pick()}>选择图片</button></div><p className={s.formHint}>PNG / JPEG / WebP，原图存入本地；移动原文件不会影响记录。单图最多 50 MiB / 4000 万像素，单条最多 30 张。</p><div className={s.editorPhotos}>{draft.imageIds.map((id,index)=>{const image=images.find(i=>i.id===id)!;return <div key={id} data-editor-image={id}><Photo image={image}/><div>{index>0&&<button type="button" onClick={()=>moveImage(index)}>上移</button>}<button type="button" aria-label={`移除图片 ${index+1}`} onClick={()=>setDraft(d=>({...d,imageIds:d.imageIds.filter(i=>i!==id)}))}>移除</button></div></div>;})}</div>
  </EditorFrame>{deletion&&event&&<Modal title="删除生活事件" onClose={closeDelete}><p className={s.modalIntro}>删除「{event.title}」及其 {event.images.length} 张图片的引用？只清理未被其他记录引用的资源，关联的目标和任务会保留。</p><div className={s.exitActions}><button className={s.dangerButton} disabled={props.busy} onClick={()=>void remove()}>确认删除事件</button><button className={s.secondaryButton} onClick={closeDelete}>保留记录</button></div></Modal>}</>;
}
