/**
 * PatientSearch - Search and select patients for encounters
 *
 * Provides:
 * - Search input with debounce
 * - Patient list with name and MRN
 * - Selection callback
 */

import React, { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Patient } from '../types';

interface PatientSearchProps {
  onSelectPatient: (patient: Patient) => void;
  onCancel?: () => void;
}

export function PatientSearch({ onSelectPatient, onCancel }: PatientSearchProps) {
  const [query, setQuery] = useState('');
  const [patients, setPatients] = useState<Patient[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounceRef = useRef<NodeJS.Timeout | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Debounced search
  useEffect(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    if (query.trim().length < 2) {
      setPatients([]);
      return;
    }

    debounceRef.current = setTimeout(async () => {
      await searchPatients(query);
    }, 300);

    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, [query]);

  const searchPatients = async (searchQuery: string) => {
    try {
      setIsLoading(true);
      setError(null);
      const results = await invoke<Patient[]>('medplum_search_patients', { query: searchQuery });
      setPatients(results);
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : String(e);
      setError(errorMsg);
      console.error('Patient search error:', e);
    } finally {
      setIsLoading(false);
    }
  };

  const handleSelect = useCallback((patient: Patient) => {
    onSelectPatient(patient);
  }, [onSelectPatient]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape' && onCancel) {
      onCancel();
    }
  };

  return (
    <div className="patient-search" onKeyDown={handleKeyDown}>
      <div className="patient-search-header">
        <h3>Select Patient</h3>
        {onCancel && (
          <button className="close-button" onClick={onCancel} title="Close">
            &times;
          </button>
        )}
      </div>

      <div className="patient-search-input-wrapper">
        <input
          ref={inputRef}
          type="text"
          className="patient-search-input"
          placeholder="Search by name or MRN..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {isLoading && <span className="search-spinner" />}
      </div>

      {error && (
        <div className="patient-search-error">
          {error}
        </div>
      )}

      <div className="patient-list">
        {patients.length === 0 && query.length >= 2 && !isLoading && (
          <div className="patient-list-empty">
            No patients found
          </div>
        )}

        {patients.map((patient) => (
          <button
            key={patient.id}
            className="patient-item"
            onClick={() => handleSelect(patient)}
          >
            <div className="patient-name">{patient.name}</div>
            {patient.mrn && (
              <div className="patient-mrn">MRN: {patient.mrn}</div>
            )}
            {patient.birthDate && (
              <div className="patient-dob">DOB: {patient.birthDate}</div>
            )}
          </button>
        ))}
      </div>

      {query.length < 2 && (
        <div className="patient-search-hint">
          Enter at least 2 characters to search
        </div>
      )}
    </div>
  );
}

export default PatientSearch;
