/**
 * 错误处理工具函数
 * 将任意 catch 到的值统一转换为 Error 对象，避免散落重复的 instanceof 判断。
 */

export function toError(err: unknown): Error {
  if (err instanceof Error) return err;
  if (typeof err === 'string') return new Error(err);
  // Tauri 拒绝时给的是 { code, message } 普通对象，直接 String() 只会得到
  // "[object Object]"，所以优先取 message 字段。
  if (err && typeof err === 'object') {
    const message = (err as { message?: unknown }).message;
    if (typeof message === 'string' && message.trim()) return new Error(message);
  }
  return new Error(String(err));
}
