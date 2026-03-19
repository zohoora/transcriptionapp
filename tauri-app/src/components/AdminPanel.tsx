import React, { useState, useCallback } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useAdminPanel } from '../hooks/useAdminPanel';
import type { PhysicianProfile, Room } from '../types';

/** Format ISO date string to a readable local date */
function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return iso;
  }
}

type Tab = 'physicians' | 'rooms';

interface PhysicianEditState {
  id: string;
  name: string;
  specialty: string;
}

interface RoomEditState {
  id: string;
  name: string;
  description: string;
}

const AdminPanel: React.FC = () => {
  const {
    physicians,
    rooms,
    loading,
    error,
    refresh,
    createPhysician,
    updatePhysician,
    deletePhysician,
    createRoom,
    updateRoom,
    deleteRoom,
  } = useAdminPanel();

  const [tab, setTab] = useState<Tab>('physicians');

  // --- Physician state ---
  const [showAddPhysician, setShowAddPhysician] = useState(false);
  const [newPhysName, setNewPhysName] = useState('');
  const [newPhysSpecialty, setNewPhysSpecialty] = useState('');
  const [addPhysError, setAddPhysError] = useState<string | null>(null);
  const [addingPhys, setAddingPhys] = useState(false);
  const [physEdit, setPhysEdit] = useState<PhysicianEditState | null>(null);
  const [physEditError, setPhysEditError] = useState<string | null>(null);
  const [physSaving, setPhysSaving] = useState(false);
  const [physDeleteTarget, setPhysDeleteTarget] = useState<PhysicianProfile | null>(null);
  const [physDeleting, setPhysDeleting] = useState(false);
  const [physDeleteError, setPhysDeleteError] = useState<string | null>(null);

  // --- Room state ---
  const [showAddRoom, setShowAddRoom] = useState(false);
  const [newRoomName, setNewRoomName] = useState('');
  const [newRoomDesc, setNewRoomDesc] = useState('');
  const [addRoomError, setAddRoomError] = useState<string | null>(null);
  const [addingRoom, setAddingRoom] = useState(false);
  const [roomEdit, setRoomEdit] = useState<RoomEditState | null>(null);
  const [roomEditError, setRoomEditError] = useState<string | null>(null);
  const [roomSaving, setRoomSaving] = useState(false);
  const [roomDeleteTarget, setRoomDeleteTarget] = useState<Room | null>(null);
  const [roomDeleting, setRoomDeleting] = useState(false);
  const [roomDeleteError, setRoomDeleteError] = useState<string | null>(null);

  const handleClose = useCallback(async () => {
    const win = getCurrentWindow();
    await win.close();
  }, []);

  // --- Physician handlers ---
  const handleAddPhys = useCallback(async () => {
    if (!newPhysName.trim()) { setAddPhysError('Name is required'); return; }
    setAddingPhys(true);
    setAddPhysError(null);
    try {
      await createPhysician(newPhysName.trim(), newPhysSpecialty.trim() || undefined);
      setNewPhysName('');
      setNewPhysSpecialty('');
      setShowAddPhysician(false);
    } catch (e) {
      setAddPhysError(e instanceof Error ? e.message : String(e));
    } finally {
      setAddingPhys(false);
    }
  }, [newPhysName, newPhysSpecialty, createPhysician]);

  const handleCancelAddPhys = useCallback(() => {
    setShowAddPhysician(false);
    setNewPhysName('');
    setNewPhysSpecialty('');
    setAddPhysError(null);
  }, []);

  const handleStartPhysEdit = useCallback((p: PhysicianProfile) => {
    setPhysEdit({ id: p.id, name: p.name, specialty: p.specialty || '' });
    setPhysEditError(null);
  }, []);

  const handleSavePhysEdit = useCallback(async () => {
    if (!physEdit) return;
    if (!physEdit.name.trim()) { setPhysEditError('Name is required'); return; }
    setPhysSaving(true);
    setPhysEditError(null);
    try {
      await updatePhysician(physEdit.id, {
        name: physEdit.name.trim(),
        specialty: physEdit.specialty.trim() || null,
      });
      setPhysEdit(null);
    } catch (e) {
      setPhysEditError(e instanceof Error ? e.message : String(e));
    } finally {
      setPhysSaving(false);
    }
  }, [physEdit, updatePhysician]);

  const handleConfirmPhysDelete = useCallback(async () => {
    if (!physDeleteTarget) return;
    setPhysDeleting(true);
    setPhysDeleteError(null);
    try {
      await deletePhysician(physDeleteTarget.id);
      setPhysDeleteTarget(null);
    } catch (e) {
      setPhysDeleteError(e instanceof Error ? e.message : String(e));
    } finally {
      setPhysDeleting(false);
    }
  }, [physDeleteTarget, deletePhysician]);

  // --- Room handlers ---
  const handleAddRoom = useCallback(async () => {
    if (!newRoomName.trim()) { setAddRoomError('Name is required'); return; }
    setAddingRoom(true);
    setAddRoomError(null);
    try {
      await createRoom(newRoomName.trim(), newRoomDesc.trim() || undefined);
      setNewRoomName('');
      setNewRoomDesc('');
      setShowAddRoom(false);
    } catch (e) {
      setAddRoomError(e instanceof Error ? e.message : String(e));
    } finally {
      setAddingRoom(false);
    }
  }, [newRoomName, newRoomDesc, createRoom]);

  const handleCancelAddRoom = useCallback(() => {
    setShowAddRoom(false);
    setNewRoomName('');
    setNewRoomDesc('');
    setAddRoomError(null);
  }, []);

  const handleStartRoomEdit = useCallback((r: Room) => {
    setRoomEdit({ id: r.id, name: r.name, description: r.description || '' });
    setRoomEditError(null);
  }, []);

  const handleSaveRoomEdit = useCallback(async () => {
    if (!roomEdit) return;
    if (!roomEdit.name.trim()) { setRoomEditError('Name is required'); return; }
    setRoomSaving(true);
    setRoomEditError(null);
    try {
      await updateRoom(roomEdit.id, {
        name: roomEdit.name.trim(),
        description: roomEdit.description.trim() || null,
      });
      setRoomEdit(null);
    } catch (e) {
      setRoomEditError(e instanceof Error ? e.message : String(e));
    } finally {
      setRoomSaving(false);
    }
  }, [roomEdit, updateRoom]);

  const handleConfirmRoomDelete = useCallback(async () => {
    if (!roomDeleteTarget) return;
    setRoomDeleting(true);
    setRoomDeleteError(null);
    try {
      await deleteRoom(roomDeleteTarget.id);
      setRoomDeleteTarget(null);
    } catch (e) {
      setRoomDeleteError(e instanceof Error ? e.message : String(e));
    } finally {
      setRoomDeleting(false);
    }
  }, [roomDeleteTarget, deleteRoom]);

  // --- Delete target for the overlay (physician or room) ---
  const deleteTarget = physDeleteTarget || roomDeleteTarget;

  return (
    <div className="admin-panel">
      {/* Header */}
      <div className="admin-panel-header">
        <div className="admin-panel-header-left">
          <h1>Admin Panel</h1>
        </div>
        <div className="admin-panel-header-right">
          <button
            className="admin-btn admin-btn-secondary"
            onClick={refresh}
            disabled={loading}
          >
            Refresh
          </button>
          <button className="admin-btn admin-btn-close" onClick={handleClose}>
            Close
          </button>
        </div>
      </div>

      {/* Tabs */}
      <div className="admin-tabs">
        <button
          className={`admin-tab ${tab === 'physicians' ? 'admin-tab-active' : ''}`}
          onClick={() => setTab('physicians')}
        >
          Physicians
          <span className="admin-tab-count">{physicians.length}</span>
        </button>
        <button
          className={`admin-tab ${tab === 'rooms' ? 'admin-tab-active' : ''}`}
          onClick={() => setTab('rooms')}
        >
          Rooms
          <span className="admin-tab-count">{rooms.length}</span>
        </button>
      </div>

      {/* Error banner */}
      {error && (
        <div className="admin-panel-error">{error}</div>
      )}

      {/* Content */}
      <div className="admin-panel-content">
        {loading ? (
          <div className="admin-panel-loading">Loading...</div>
        ) : tab === 'physicians' ? (
          <>
            {/* Add physician */}
            {!showAddPhysician ? (
              <div className="admin-panel-actions">
                <button
                  className="admin-btn admin-btn-primary"
                  onClick={() => setShowAddPhysician(true)}
                >
                  + Add Physician
                </button>
              </div>
            ) : (
              <div className="admin-form-card">
                <h3 className="admin-form-title">Add Physician</h3>
                <div className="admin-form-row">
                  <label className="admin-form-label">Name *</label>
                  <input
                    className="admin-form-input"
                    type="text"
                    value={newPhysName}
                    onChange={(e) => setNewPhysName(e.target.value)}
                    placeholder="Dr. Jane Smith"
                    autoFocus
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleAddPhys();
                      if (e.key === 'Escape') handleCancelAddPhys();
                    }}
                  />
                </div>
                <div className="admin-form-row">
                  <label className="admin-form-label">Specialty</label>
                  <input
                    className="admin-form-input"
                    type="text"
                    value={newPhysSpecialty}
                    onChange={(e) => setNewPhysSpecialty(e.target.value)}
                    placeholder="Family Medicine"
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleAddPhys();
                      if (e.key === 'Escape') handleCancelAddPhys();
                    }}
                  />
                </div>
                {addPhysError && <div className="admin-form-error">{addPhysError}</div>}
                <div className="admin-form-buttons">
                  <button className="admin-btn admin-btn-secondary" onClick={handleCancelAddPhys} disabled={addingPhys}>Cancel</button>
                  <button className="admin-btn admin-btn-primary" onClick={handleAddPhys} disabled={addingPhys || !newPhysName.trim()}>
                    {addingPhys ? 'Creating...' : 'Create'}
                  </button>
                </div>
              </div>
            )}

            {/* Physician table */}
            {physicians.length === 0 ? (
              <div className="admin-panel-empty">No physicians found. Add one to get started.</div>
            ) : (
              <div className="admin-table-wrapper">
                <table className="admin-table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>Specialty</th>
                      <th>Created</th>
                      <th className="admin-table-actions-col">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {physicians.map((physician) =>
                      physEdit && physEdit.id === physician.id ? (
                        <tr key={physician.id} className="admin-table-edit-row">
                          <td>
                            <input
                              className="admin-form-input admin-table-input"
                              type="text"
                              value={physEdit.name}
                              onChange={(e) => setPhysEdit((prev) => prev ? { ...prev, name: e.target.value } : prev)}
                              autoFocus
                              onKeyDown={(e) => {
                                if (e.key === 'Enter') handleSavePhysEdit();
                                if (e.key === 'Escape') { setPhysEdit(null); setPhysEditError(null); }
                              }}
                            />
                          </td>
                          <td>
                            <input
                              className="admin-form-input admin-table-input"
                              type="text"
                              value={physEdit.specialty}
                              onChange={(e) => setPhysEdit((prev) => prev ? { ...prev, specialty: e.target.value } : prev)}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter') handleSavePhysEdit();
                                if (e.key === 'Escape') { setPhysEdit(null); setPhysEditError(null); }
                              }}
                            />
                          </td>
                          <td className="admin-table-date">{formatDate(physician.created_at)}</td>
                          <td className="admin-table-actions">
                            {physEditError && <span className="admin-inline-error">{physEditError}</span>}
                            <button className="admin-btn admin-btn-small admin-btn-secondary" onClick={() => { setPhysEdit(null); setPhysEditError(null); }} disabled={physSaving}>Cancel</button>
                            <button className="admin-btn admin-btn-small admin-btn-primary" onClick={handleSavePhysEdit} disabled={physSaving || !physEdit.name.trim()}>
                              {physSaving ? 'Saving...' : 'Save'}
                            </button>
                          </td>
                        </tr>
                      ) : (
                        <tr key={physician.id}>
                          <td className="admin-table-name">{physician.name}</td>
                          <td className="admin-table-specialty">{physician.specialty || '--'}</td>
                          <td className="admin-table-date">{formatDate(physician.created_at)}</td>
                          <td className="admin-table-actions">
                            <button className="admin-btn admin-btn-small admin-btn-edit" onClick={() => handleStartPhysEdit(physician)}>Edit</button>
                            <button className="admin-btn admin-btn-small admin-btn-danger" onClick={() => setPhysDeleteTarget(physician)}>Delete</button>
                          </td>
                        </tr>
                      )
                    )}
                  </tbody>
                </table>
              </div>
            )}
          </>
        ) : (
          /* ===== Rooms tab ===== */
          <>
            {/* Add room */}
            {!showAddRoom ? (
              <div className="admin-panel-actions">
                <button
                  className="admin-btn admin-btn-primary"
                  onClick={() => setShowAddRoom(true)}
                >
                  + Add Room
                </button>
              </div>
            ) : (
              <div className="admin-form-card">
                <h3 className="admin-form-title">Add Room</h3>
                <div className="admin-form-row">
                  <label className="admin-form-label">Name *</label>
                  <input
                    className="admin-form-input"
                    type="text"
                    value={newRoomName}
                    onChange={(e) => setNewRoomName(e.target.value)}
                    placeholder="Exam Room 3"
                    autoFocus
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleAddRoom();
                      if (e.key === 'Escape') handleCancelAddRoom();
                    }}
                  />
                </div>
                <div className="admin-form-row">
                  <label className="admin-form-label">Description</label>
                  <input
                    className="admin-form-input"
                    type="text"
                    value={newRoomDesc}
                    onChange={(e) => setNewRoomDesc(e.target.value)}
                    placeholder="Second floor, east wing"
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleAddRoom();
                      if (e.key === 'Escape') handleCancelAddRoom();
                    }}
                  />
                </div>
                {addRoomError && <div className="admin-form-error">{addRoomError}</div>}
                <div className="admin-form-buttons">
                  <button className="admin-btn admin-btn-secondary" onClick={handleCancelAddRoom} disabled={addingRoom}>Cancel</button>
                  <button className="admin-btn admin-btn-primary" onClick={handleAddRoom} disabled={addingRoom || !newRoomName.trim()}>
                    {addingRoom ? 'Creating...' : 'Create'}
                  </button>
                </div>
              </div>
            )}

            {/* Room table */}
            {rooms.length === 0 ? (
              <div className="admin-panel-empty">No rooms found. Add one to get started.</div>
            ) : (
              <div className="admin-table-wrapper">
                <table className="admin-table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>Description</th>
                      <th>Created</th>
                      <th className="admin-table-actions-col">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rooms.map((room) =>
                      roomEdit && roomEdit.id === room.id ? (
                        <tr key={room.id} className="admin-table-edit-row">
                          <td>
                            <input
                              className="admin-form-input admin-table-input"
                              type="text"
                              value={roomEdit.name}
                              onChange={(e) => setRoomEdit((prev) => prev ? { ...prev, name: e.target.value } : prev)}
                              autoFocus
                              onKeyDown={(e) => {
                                if (e.key === 'Enter') handleSaveRoomEdit();
                                if (e.key === 'Escape') { setRoomEdit(null); setRoomEditError(null); }
                              }}
                            />
                          </td>
                          <td>
                            <input
                              className="admin-form-input admin-table-input"
                              type="text"
                              value={roomEdit.description}
                              onChange={(e) => setRoomEdit((prev) => prev ? { ...prev, description: e.target.value } : prev)}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter') handleSaveRoomEdit();
                                if (e.key === 'Escape') { setRoomEdit(null); setRoomEditError(null); }
                              }}
                            />
                          </td>
                          <td className="admin-table-date">{formatDate(room.created_at)}</td>
                          <td className="admin-table-actions">
                            {roomEditError && <span className="admin-inline-error">{roomEditError}</span>}
                            <button className="admin-btn admin-btn-small admin-btn-secondary" onClick={() => { setRoomEdit(null); setRoomEditError(null); }} disabled={roomSaving}>Cancel</button>
                            <button className="admin-btn admin-btn-small admin-btn-primary" onClick={handleSaveRoomEdit} disabled={roomSaving || !roomEdit.name.trim()}>
                              {roomSaving ? 'Saving...' : 'Save'}
                            </button>
                          </td>
                        </tr>
                      ) : (
                        <tr key={room.id}>
                          <td className="admin-table-name">{room.name}</td>
                          <td className="admin-table-specialty">{room.description || '--'}</td>
                          <td className="admin-table-date">{formatDate(room.created_at)}</td>
                          <td className="admin-table-actions">
                            <button className="admin-btn admin-btn-small admin-btn-edit" onClick={() => handleStartRoomEdit(room)}>Edit</button>
                            <button className="admin-btn admin-btn-small admin-btn-danger" onClick={() => setRoomDeleteTarget(room)}>Delete</button>
                          </td>
                        </tr>
                      )
                    )}
                  </tbody>
                </table>
              </div>
            )}
          </>
        )}
      </div>

      {/* Delete confirmation dialog (shared for physicians and rooms) */}
      {deleteTarget && (
        <div className="admin-dialog-overlay">
          <div className="admin-dialog">
            <h3 className="admin-dialog-title">
              Delete {physDeleteTarget ? 'Physician' : 'Room'}
            </h3>
            <p className="admin-dialog-message">
              Are you sure you want to delete <strong>{physDeleteTarget?.name || roomDeleteTarget?.name}</strong>?
              This action cannot be undone.
            </p>
            {(physDeleteError || roomDeleteError) && (
              <div className="admin-form-error">{physDeleteError || roomDeleteError}</div>
            )}
            <div className="admin-dialog-buttons">
              <button
                className="admin-btn admin-btn-secondary"
                onClick={() => {
                  setPhysDeleteTarget(null);
                  setPhysDeleteError(null);
                  setRoomDeleteTarget(null);
                  setRoomDeleteError(null);
                }}
                disabled={physDeleting || roomDeleting}
              >
                Cancel
              </button>
              <button
                className="admin-btn admin-btn-danger"
                onClick={physDeleteTarget ? handleConfirmPhysDelete : handleConfirmRoomDelete}
                disabled={physDeleting || roomDeleting}
              >
                {(physDeleting || roomDeleting) ? 'Deleting...' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default AdminPanel;
