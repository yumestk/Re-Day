import { invoke } from '@tauri-apps/api/core';

export type Slot = 'morning' | 'afternoon' | 'evening';
export const slots: { id: Slot; label: string; english: string; number: string }[] = [
  { id: 'morning', label: '上午', english: 'MORNING', number: '01' },
  { id: 'afternoon', label: '下午', english: 'AFTERNOON', number: '02' },
  { id: 'evening', label: '晚上', english: 'EVENING', number: '03' },
];

export interface Preferences {
  soundEnabled: boolean;
  reducedMotion: boolean;
  revision: number;
}

export interface TaskItem {
  planId: string;
  questId: string;
  title: string;
  description: string;
  currentTitle: string;
  currentDescription: string;
  scheduledOn: string;
  slot: Slot;
  startTime: string | null;
  endTime: string | null;
  position: number;
  completed: boolean;
  completedOn: string | null;
  completionId: string | null;
  revision: number;
  questRevision: number;
}

export interface DayView {
  datasetId: string;
  date: string;
  today: string;
  items: TaskItem[];
  preferences: Preferences;
}

export interface TaskDraft {
  title: string;
  description: string;
  date: string;
  slot: Slot;
  startTime: string | null;
  endTime: string | null;
}

export interface MutationResult { item: TaskItem; changed: boolean }
export interface AppError { code: string; message: string }

export function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error && 'message' in error) return String(error.message);
  return typeof error === 'string' ? error : '保存未完成，请重试。';
}

// The only IPC boundary: components deal with task intentions, never SQL or paths.
export const api = {
  loadDay: (date: string) => invoke<DayView>('load_day', { date }),
  createQuest: (datasetId: string, draft: TaskDraft, ids: { questId: string; planId: string }) =>
    invoke<MutationResult>('create_quest', { request: { datasetId, ...ids, ...draft } }),
  editQuest: (datasetId: string, item: TaskItem, draft: TaskDraft) =>
    invoke<MutationResult>('edit_quest', { request: {
      datasetId, planId: item.planId, expectedRevision: item.revision,
      expectedQuestRevision: item.questRevision, ...draft,
    } }),
  setCompletion: (datasetId: string, item: TaskItem, desiredCompleted: boolean, recordId: string) =>
    invoke<MutationResult>('set_completion', { request: {
      datasetId, planId: item.planId, desiredCompleted, recordId,
      completionId: item.completionId, expectedRevision: item.revision,
      expectedQuestRevision: item.questRevision,
    } }),
  savePreferences: (datasetId: string, preferences: Preferences) =>
    invoke<Preferences>('save_preferences', { request: { datasetId, ...preferences } }),
};
