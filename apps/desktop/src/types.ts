export interface NoteStatus {
  proven: number;
  unstable: number;
  discarded: number;
  practice: number;
  opposed: number;
  unknown: number;
  evidence: string;
}

export interface AgentContext {
  request: string;
  independent: string;
  sources: string;
  note: string;
  existing: number;
  suggestion: string;
}

export interface Note {
  id: string;
  title: string;
  kind: string;
  domains: string;
  updated: string;
  nextStep: string;
  intro: string;
  goal: string;
  status: NoteStatus;
  nextSteps: string[];
  materials: string[];
  proven: string[];
  unstable: string[];
  agent: AgentContext;
}

export interface EventRow {
  id: number;
  at: string;
  text: string;
}

export interface HostInfo {
  version: string;
  dataDir: string;
  dbPath: string;
  sandboxRoot: string;
  platform: string;
}
