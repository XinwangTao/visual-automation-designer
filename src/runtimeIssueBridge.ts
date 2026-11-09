import { logRuntimeIssue } from './tauri/logging';

/** AppShell 消费的运行时异常事件名。 */
export const RUNTIME_ERROR_EVENT = 'vad:runtime-error';

export interface RuntimeErrorDetail {
  source: string;
  error: string;
}

let refCount = 0;
let teardown: (() => void) | null = null;

function report(source: string, message: string, details?: string): void {
  // 上报失败不能反过来制造新的异常：logRuntimeIssue 内部已吞掉后端不可用。
  void logRuntimeIssue({ source, message, details });

  if (typeof window === 'undefined') {
    return;
  }
  window.dispatchEvent(
    new CustomEvent<RuntimeErrorDetail>(RUNTIME_ERROR_EVENT, {
      detail: { source, error: message },
    }),
  );
}

function attach(): () => void {
  const handleError = (event: ErrorEvent) => {
    const details = event.error instanceof Error && event.error.stack ? event.error.stack : undefined;
    report('window.error', event.message, details);
  };

  const handleRejection = (event: PromiseRejectionEvent) => {
    const reason = event.reason;
    const message = reason instanceof Error ? reason.message : String(reason);
    const details = reason instanceof Error && reason.stack ? reason.stack : undefined;
    report('window.unhandledrejection', message, details);
  };

  window.addEventListener('error', handleError);
  window.addEventListener('unhandledrejection', handleRejection);

  return () => {
    window.removeEventListener('error', handleError);
    window.removeEventListener('unhandledrejection', handleRejection);
  };
}

/**
 * 安装「原生错误 → vad:runtime-error」桥。
 *
 * 之前只在 main.tsx 里注册原生监听，AppShell 依赖进程入口把它装上：一旦 App 被嵌入
 * 其它入口（测试、Storybook、浏览器模式预览）或入口被改动，前端异常就完全静默——
 * 界面上既不弹 toast 也不进执行日志。这里改成幂等安装 + 引用计数：谁挂载谁安装，
 * 最后一个卸载者负责拆掉监听，不会重复注册。
 */
export function installRuntimeIssueBridge(): () => void {
  if (typeof window === 'undefined') {
    return () => {};
  }

  if (!teardown) {
    teardown = attach();
  }
  refCount += 1;

  let released = false;
  return () => {
    if (released) {
      return;
    }
    released = true;
    refCount -= 1;
    if (refCount <= 0) {
      refCount = 0;
      teardown?.();
      teardown = null;
    }
  };
}
