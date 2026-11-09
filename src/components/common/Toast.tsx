/**
 * Toast - 通知提示组件
 * 显示操作成功/失败/信息提示
 */

import { createContext, useContext, useState, useCallback, useRef, useEffect, ReactNode } from 'react';
import commonStyles from './common.module.css';

export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface ToastMessage {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
}

interface ToastContextValue {
  toasts: ToastMessage[];
  showToast: (type: ToastType, message: string, duration?: number) => void;
  removeToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast must be used within ToastProvider');
  }
  return context;
}

interface ToastProviderProps {
  children: ReactNode;
}

export function ToastProvider({ children }: ToastProviderProps) {
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const timeoutIdsRef = useRef<ReturnType<typeof setTimeout>[]>([]);

  // Cleanup all pending timeouts on unmount
  useEffect(() => {
    return () => {
      timeoutIdsRef.current.forEach((id) => clearTimeout(id));
      timeoutIdsRef.current.length = 0;
    };
  }, []);

  const showToast = useCallback((type: ToastType, message: string, duration = 3000) => {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
    const newToast: ToastMessage = { id, type, message, duration };

    setToasts((prev) => [...prev, newToast]);

    // Auto remove after duration
    if (duration > 0) {
      const timeoutId = setTimeout(() => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
        timeoutIdsRef.current = timeoutIdsRef.current.filter((tid) => tid !== timeoutId);
      }, duration);
      timeoutIdsRef.current.push(timeoutId);
    }
  }, []);

  const removeToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ toasts, showToast, removeToast }}>
      {children}
      <ToastContainer toasts={toasts} onRemove={removeToast} />
    </ToastContext.Provider>
  );
}

interface ToastContainerProps {
  toasts: ToastMessage[];
  onRemove: (id: string) => void;
}

function ToastContainer({ toasts, onRemove }: ToastContainerProps) {
  if (toasts.length === 0) return null;

  return (
    <div className="toast-container">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onRemove={onRemove} />
      ))}
    </div>
  );
}

interface ToastItemProps {
  toast: ToastMessage;
  onRemove: (id: string) => void;
}

function ToastGlyph({ type }: { type: ToastMessage['type'] }) {
  const path =
    type === 'success'
      ? 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12zM5.2 8.2l1.9 1.9 3.7-4'
      : type === 'error'
        ? 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12zM5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4'
        : type === 'warning'
          ? 'M8 2.2 14.4 13H1.6zM8 6v3.4M8 11.2v.2'
          : 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12zM8 7.2v3.4M8 4.8v.2';
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={path} />
    </svg>
  );
}

function ToastItem({ toast, onRemove }: ToastItemProps) {
  return (
    <div className={`${commonStyles.toast} ${commonStyles[`toast${toast.type.charAt(0).toUpperCase() + toast.type.slice(1)}`]}`} role="status" aria-live="polite">
      <span className={commonStyles.toastIcon}><ToastGlyph type={toast.type} /></span>
      <span className={commonStyles.toastMessage}>{toast.message}</span>
      <button
        className={commonStyles.toastClose}
        onClick={() => onRemove(toast.id)}
        type="button"
        aria-label="关闭"
      >
        ×
      </button>
    </div>
  );
}

export default ToastProvider;
