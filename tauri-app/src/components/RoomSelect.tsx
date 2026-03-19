import { memo, useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Room } from '../types';

interface RoomSelectProps {
  profileServerUrl: string | null;
  onSelect: (room: Room) => void;
}

/**
 * Full-screen room selection grid, similar to PhysicianSelect.
 * Lists rooms from the profile server. Allows creating new rooms inline.
 */
export const RoomSelect = memo(function RoomSelect({
  profileServerUrl,
  onSelect,
}: RoomSelectProps) {
  const [rooms, setRooms] = useState<Room[]>([]);
  const [loading, setLoading] = useState(true);
  const [showAdd, setShowAdd] = useState(false);
  const [newName, setNewName] = useState('');
  const [newDescription, setNewDescription] = useState('');
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<Room[]>('get_rooms');
      setRooms(list);
    } catch (e) {
      console.error('Failed to load rooms:', e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  // Refresh on window focus (e.g. after admin panel changes)
  useEffect(() => {
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  }, [refresh]);

  const handleCreate = useCallback(async () => {
    if (!newName.trim()) return;
    setCreating(true);
    setError(null);
    try {
      const room = await invoke<Room>('create_room', {
        name: newName.trim(),
        description: newDescription.trim() || null,
      });
      setRooms(prev => [...prev, room]);
      setNewName('');
      setNewDescription('');
      setShowAdd(false);
    } catch (e) {
      console.error('Failed to create room:', e);
      setError(String(e));
    } finally {
      setCreating(false);
    }
  }, [newName, newDescription]);

  if (loading) {
    return (
      <div className="physician-select-overlay">
        <div className="physician-select-card">
          <h2 className="physician-select-title">Select Room</h2>
          <div className="physician-select-loading">Loading rooms...</div>
        </div>
      </div>
    );
  }

  return (
    <div className="physician-select-overlay">
      <div className="physician-select-card">
        <div className="physician-select-header-row">
          <div>
            <h2 className="physician-select-title">Select Room</h2>
            <p className="physician-select-subtitle">
              Choose the room for this workstation
            </p>
            {profileServerUrl && (
              <p style={{ fontSize: 11, opacity: 0.4, margin: '4px 0 0' }}>
                {profileServerUrl}
              </p>
            )}
          </div>
          <button
            className="admin-link-btn"
            onClick={() => setShowAdd(!showAdd)}
          >
            {showAdd ? 'Cancel' : '+ New Room'}
          </button>
        </div>

        {showAdd && (
          <div style={{ margin: '16px 0', padding: 16, background: '#f8f9fa', borderRadius: 8 }}>
            <div style={{ marginBottom: 8 }}>
              <input
                type="text"
                className="room-setup-input"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="Room name (e.g. Exam Room 3)"
                autoFocus
                style={{ width: '100%', marginBottom: 8 }}
              />
              <input
                type="text"
                className="room-setup-input"
                value={newDescription}
                onChange={(e) => setNewDescription(e.target.value)}
                placeholder="Description (optional)"
                style={{ width: '100%' }}
              />
            </div>
            {error && <div style={{ color: '#e53e3e', fontSize: 12, marginBottom: 8 }}>{error}</div>}
            <button
              className="room-setup-save-btn"
              onClick={handleCreate}
              disabled={!newName.trim() || creating}
              style={{ width: 'auto', padding: '8px 20px' }}
            >
              {creating ? 'Creating...' : 'Create Room'}
            </button>
          </div>
        )}

        {rooms.length === 0 && !showAdd ? (
          <div className="physician-select-empty">
            No rooms found. Click "+ New Room" to create one.
          </div>
        ) : (
          <div className="physician-grid">
            {rooms.map((room) => (
              <button
                key={room.id}
                className="physician-card"
                onClick={() => onSelect(room)}
              >
                <span className="physician-card-name">{room.name}</span>
                {room.description && (
                  <span className="physician-card-specialty">{room.description}</span>
                )}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
});

export default RoomSelect;
