import { invoke } from '@tauri-apps/api/core';

export interface RuntimeIssuePayload {
  source: string;
  message: string;
  details?: string;
}

export async function logRuntimeIssue(payload: RuntimeIssuePayload): Promise<void> {
  try {
    await invoke('log_runtime_issue', { payload });
  } catch {
    // 后端不可用（浏览器模式、单测、后端命令尚未注册）时静默丢弃：
    // 这是兜底上报通道，它自己再抛一次会变成 unhandledrejection，反过来触发整条错误链。
  }
}
