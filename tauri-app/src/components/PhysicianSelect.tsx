import { memo, useCallback, useEffect } from 'react';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { PhysicianProfile } from '../types';

interface PhysicianSelectProps {
  physicians: PhysicianProfile[];
  loading: boolean;
  onSelect: (id: string) => void;
  onRefresh?: () => void;
}

/**
 * Full-screen physician selection grid.
 * Touch-friendly cards showing physician name and specialty.
 */
export const PhysicianSelect = memo(function PhysicianSelect({
  physicians,
  loading,
  onSelect,
  onRefresh,
}: PhysicianSelectProps) {
  // Refresh physician list when window regains focus (e.g. after admin panel closes)
  useEffect(() => {
    if (!onRefresh) return;
    const handler = () => onRefresh();
    window.addEventListener('focus', handler);
    return () => window.removeEventListener('focus', handler);
  }, [onRefresh]);

  const openAdminPanel = useCallback(async () => {
    try {
      const existing = await WebviewWindow.getByLabel('admin');
      if (existing) {
        await existing.setFocus();
        return;
      }
      const adminWindow = new WebviewWindow('admin', {
        url: 'admin.html',
        title: 'Admin Panel',
        width: 800,
        height: 600,
        minWidth: 600,
        minHeight: 400,
        resizable: true,
      });
      adminWindow.once('tauri://error', (e) => {
        console.error('Failed to open admin window:', e);
      });
    } catch (e) {
      console.error('Failed to open admin panel:', e);
    }
  }, []);

  if (loading) {
    return (
      <div className="physician-select-overlay">
        <div className="physician-select-card">
          <h2 className="physician-select-title">Select Physician</h2>
          <div className="physician-select-loading">Loading physicians...</div>
        </div>
      </div>
    );
  }

  if (physicians.length === 0) {
    return (
      <div className="physician-select-overlay">
        <div className="physician-select-card">
          <h2 className="physician-select-title">Select Physician</h2>
          <div className="physician-select-empty">
            No physicians found. Please check the profile server connection.
          </div>
          <div className="physician-select-admin">
            <button
              className="admin-link-btn"
              onClick={openAdminPanel}
            >
              Open Admin Panel
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="physician-select-overlay">
      <div className="physician-select-card">
        <div className="physician-select-header-row">
          <div>
            <h2 className="physician-select-title">Select Physician</h2>
            <p className="physician-select-subtitle">
              Choose the physician using this workstation
            </p>
          </div>
          <button
            className="admin-link-btn"
            onClick={openAdminPanel}
          >
            Admin
          </button>
        </div>

        <div className="physician-grid">
          {physicians.map((physician) => (
            <button
              key={physician.id}
              className="physician-card"
              onClick={() => onSelect(physician.id)}
            >
              <span className="physician-card-name">{physician.name}</span>
              {physician.specialty && (
                <span className="physician-card-specialty">{physician.specialty}</span>
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
});

export default PhysicianSelect;
