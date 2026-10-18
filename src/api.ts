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
  taskMode: 'single' | 'recurring';
  origin: 'manual' | 'recurring' | 'makeup';
  occurrenceOn: string | null;
  disposition: 'active' | 'skipped' | 'rescheduled' | 'withdrawn';
  nextPlanId: string | null;
  isOverride: boolean;
  hasRecords: boolean;
  sourceOn: string | null;
  goals: GoalSnapshot[];
  canExecute: boolean;
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
  records: Execution[];
  datasetId: string;
  date: string;
  today: string;
  items: TaskItem[];
  preferences: Preferences;
}

export interface TaskDraft {
  goalIds: string[];
  parentId: string | null;
  title: string;
  description: string;
  date: string;
  slot: Slot;
  startTime: string | null;
  endTime: string | null;
}

export interface GoalSnapshot { id: string; name: string; kind: 'main' | 'side' }
export interface Goal extends GoalSnapshot { description: string; status: 'open' | 'completed'; archived: boolean; revision: number }
export interface QuestDraft { title: string; description: string; parentId: string | null; goalIds: string[] }
export interface Quest extends QuestDraft { recurrence: RuleVersion[]; id: string; mode: 'single' | 'recurring'; status: 'open' | 'completed' | 'cancelled'; archivedOn: string | null; revision: number; completionId: string | null; completedOn: string | null }
export interface Workspace { datasetId: string; today: string; goals: Goal[]; quests: Quest[] }
export interface Execution { id: string; questId: string; planId: string | null; kind: 'progress' | 'completion'; occurredOn: string; recordedAt: string; note: string | null; snapshot: { title: string; description: string; goals: GoalSnapshot[] }; revokedAt: string | null; completionScope: 'quest' | 'instance' | null }
export interface QuestHistory { records: Execution[]; plans: { id: string; date: string; slot: Slot; title: string; disposition: string; origin: string; occurrenceOn: string | null; nextPlanId: string | null }[] }
export interface DeletePreview { questId: string; title: string; plans: number; progress: number; completions: number; children: { id: string; title: string; revision: number }[]; lifeEvents: number; hasHistory: boolean; fingerprint: string }
export type RunMutation = <T>(key: string, operation: () => Promise<T>) => Promise<T | null>;
export interface ExecutionDraft { recordId: string; kind: 'progress' | 'completion' | 'undo'; occurredOn: string; note: string }

export interface RuleVersion {
  id: string; questId: string; effectiveFrom: string; effectiveUntil: string | null;
  frequency: 'daily' | 'weekly' | 'interval'; intervalDays: number | null; weekdays: number | null;
  anchorOn: string; endOn: string | null; enabled: boolean; slot: Slot;
  startTime: string | null; endTime: string | null; snapshot: Execution['snapshot'];
}
export interface RuleDraft extends TaskDraft { frequency: RuleVersion['frequency']; intervalDays: number | null; weekdays: number | null; endOn: string | null; enabled: boolean }
export interface RuleRequest extends RuleDraft { datasetId: string; questId: string; versionId: string; expectedRevision: number }
export interface RulePreview { fingerprint: string; changes: { date: string; title: string; action: string }[]; retainedOverrides: string[]; preservedFacts: number }
export interface MutationResult { item: TaskItem; changed: boolean }
export interface AppError { code: string; message: string }

export function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error && 'message' in error) return String(error.message);
  return typeof error === 'string' ? error : '保存未完成，请重试。';
}

// The only IPC boundary: components deal with task intentions, never SQL or paths.
export const api = {
  previewRule: (request: RuleRequest) => invoke<RulePreview>('preview_recurrence', { request }),
  saveRule: (request: RuleRequest, fingerprint: string) => invoke<boolean>('save_recurrence', { request: { request, fingerprint } }),
  instances: (datasetId: string, questId: string, date: string) => invoke<TaskItem[]>('load_instances', { datasetId, questId, date }),
  reschedule: (datasetId: string, item: TaskItem, makeupId: string, draft: { date: string; slot: Slot; startTime: string | null; endTime: string | null }) => invoke<MutationResult>('reschedule_instance', { request: { datasetId, planId: item.planId, makeupId, expectedRevision: item.revision, expectedQuestRevision: item.questRevision, ...draft } }),
  loadWorkspace: () => invoke<Workspace>('load_workspace'),
  saveGoal: (datasetId: string, goal: Goal, expectedRevision: number | null) => invoke<Goal>('save_goal', { request: { datasetId, goal, expectedRevision } }),
  saveQuest: (datasetId: string, questId: string, draft: QuestDraft, expectedRevision: number | null) => invoke<Quest>('save_quest', { request: { datasetId, questId, ...draft, expectedRevision } }),
  questState: (datasetId: string, quest: Quest, action: 'archive' | 'unarchive' | 'cancel' | 'reopen') => invoke<Quest>('set_quest_state', { request: { datasetId, questId: quest.id, expectedRevision: quest.revision, action } }),
  history: (questId: string) => invoke<QuestHistory>('load_quest_history', { questId }),
  record: (datasetId: string, quest: Quest, draft: ExecutionDraft, plan?: TaskItem) => invoke<boolean>('record_execution', { request: { datasetId, questId: quest.id, planId: plan?.planId ?? null, expectedRevision: quest.revision, expectedPlanRevision: plan?.revision ?? null, completionId: quest.mode === 'recurring' ? plan?.completionId ?? null : quest.completionId, ...draft } }),
  schedule: (datasetId: string, quest: Quest, planId: string, draft: TaskDraft) => invoke<TaskItem>('schedule_quest', { request: { datasetId, questId: quest.id, planId, expectedRevision: quest.revision, ...draft } }),
  previewDelete: (datasetId: string, questId: string) => invoke<DeletePreview>('preview_quest_delete', { datasetId, questId }),
  deleteQuest: (datasetId: string, preview: DeletePreview, confirmedHistory: boolean) => invoke<boolean>('delete_quest', { request: { datasetId, preview, confirmedHistory } }),
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
