import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './styles/tokens.css';
import './main.css';
import './styles/ui-refresh.css';
import './styles/liquid-glass.css';
import './styles/inspira-effects.css';
import { installRuntimeIssueBridge } from './runtimeIssueBridge';

// 进程入口先装一次；AppShell 挂载时还会按引用计数再装一次（幂等）。
// 这样「异常 → toast + 执行日志」这条链路不依赖具体入口是谁。
installRuntimeIssueBridge();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
