import { useEffect, useRef, type ReactNode } from 'react';
import s from './App.module.css';

type IconName = 'plus' | 'check' | 'arrow' | 'close' | 'sun' | 'sunset' | 'moon' | 'settings' | 'volume' | 'mute' | 'motion' | 'calendar' | 'chevron' | 'spark';
export function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
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

export function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
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
