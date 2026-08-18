import { useState } from 'react';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { openUrl } from '@tauri-apps/plugin-opener';

const RELEASES_URL = 'https://github.com/Fouxiuan/ctrip-price-tester/releases';

export async function checkForUpdates(onUpdate) {
  try {
    const update = await check();
    if (update) onUpdate(update);
  } catch {
    // portable 版不支持自动更新，或更新源不可用：打开 GitHub Releases 页面
    try {
      await openUrl(RELEASES_URL);
    } catch {
      // 无 opener 权限时静默失败，不打断主流程
    }
  }
}

export default function UpdaterDialog({ update, onClose }) {
  const [phase, setPhase] = useState('confirm');
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState('');

  const startDownload = async () => {
    setPhase('downloading');
    setError('');
    try {
      await update.download((event) => {
        if (event.event === 'Progress') setProgress(event.data.progress);
      });
      await update.install();
      await relaunch();
    } catch (reason) {
      setPhase('error');
      setError(String(reason));
    }
  };

  return (
    <div className="update-mask" role="dialog" aria-modal="true" aria-label="发现新版本">
      <div className="update-dialog">
        <div className="update-dialog-head">
          <span className="update-dialog-kicker">UPDATE AVAILABLE</span>
          <h2>{phase === 'error' ? '更新失败' : phase === 'downloading' ? '正在更新' : '发现新版本'}</h2>
        </div>

        {phase === 'confirm' && (
          <>
            <p className="update-dialog-body">
              当前版本 <strong>v{update.currentVersion}</strong>，检测到新版本 <strong>v{update.version}</strong>。
              {update.body && <span className="update-dialog-notes">{update.body}</span>}
            </p>
            <div className="update-dialog-actions">
              <button type="button" className="btn-plain" onClick={onClose}>暂不更新</button>
              <button type="button" className="btn-primary" onClick={startDownload}>立即更新</button>
            </div>
          </>
        )}

        {phase === 'downloading' && (
          <>
            <div className="update-progress" role="progressbar" aria-valuenow={Math.round(progress * 100)} aria-valuemin="0" aria-valuemax="100">
              <i style={{ width: `${Math.round(progress * 100)}%` }} />
            </div>
            <p className="update-dialog-hint">正在下载更新包… {Math.round(progress * 100)}%</p>
            <p className="update-dialog-hint small">下载完成后将自动安装并重启。</p>
          </>
        )}

        {phase === 'error' && (
          <>
            <p className="update-dialog-body">{error}</p>
            <div className="update-dialog-actions">
              <button type="button" className="btn-primary" onClick={() => openUrl(RELEASES_URL).catch(() => {})}>打开下载页</button>
              <button type="button" className="btn-plain" onClick={onClose}>关闭</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
