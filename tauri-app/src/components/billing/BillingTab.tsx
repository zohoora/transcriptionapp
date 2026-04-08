import React, { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { BillingRecord, BillingCode, BillingContext, OhipCodeSearchResult, DiagnosticCodeSearchResult, BillingCategory } from '../../types';
import { VISIT_SETTING_OPTIONS } from '../../types';
import { formatCents, confidenceBadgeClass, OHIP_CODE_CRITERIA, findConflicts, findAllConflicts } from './billingUtils';

interface BillingTabProps {
  record: BillingRecord | null;
  loading: boolean;
  sessionId: string;
  date: string;
  patientDob?: string | null;
  onRecordChange: (record: BillingRecord) => void;
  /** Global billing defaults from physician settings */
  defaultVisitSetting?: string;
  defaultCounsellingExhausted?: boolean;
  defaultIsHospital?: boolean;
}

export const BillingTab: React.FC<BillingTabProps> = ({
  record, loading, sessionId, date, patientDob, onRecordChange,
  defaultVisitSetting, defaultCounsellingExhausted, defaultIsHospital,
}) => {
  const [extracting, setExtracting] = useState(false);
  const [copied, setCopied] = useState(false);
  const [extractError, setExtractError] = useState<string | null>(null);
  const [showAddCode, setShowAddCode] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<OhipCodeSearchResult[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);

  // Diagnostic code search
  const [showDxSearch, setShowDxSearch] = useState(false);
  const [dxQuery, setDxQuery] = useState('');
  const [dxResults, setDxResults] = useState<DiagnosticCodeSearchResult[]>([]);
  const [dxSearchLoading, setDxSearchLoading] = useState(false);

  // Billing context selectors (initialized from physician billing preferences)
  const [contextExpanded, setContextExpanded] = useState(false);
  const [visitSetting, setVisitSetting] = useState(defaultVisitSetting || 'in_office');
  const [patientAge, setPatientAge] = useState('adult');
  const [referralReceived, setReferralReceived] = useState(false);
  const [counsellingExhausted, setCounsellingExhausted] = useState(defaultCounsellingExhausted || false);
  const [afterHoursMode, setAfterHoursMode] = useState<'auto' | 'yes' | 'no'>('auto');
  const [isHospital, setIsHospital] = useState(defaultIsHospital || false);

  // Auto-populate age bracket from vision-extracted DOB
  useEffect(() => {
    if (!patientDob) return;
    const birth = new Date(patientDob);
    if (isNaN(birth.getTime())) return;
    const today = new Date();
    let age = today.getFullYear() - birth.getFullYear();
    if (today.getMonth() < birth.getMonth() ||
        (today.getMonth() === birth.getMonth() && today.getDate() < birth.getDate())) {
      age--;
    }
    if (age <= 1) setPatientAge('child_0_1');
    else if (age <= 15) setPatientAge('child_2_15');
    else if (age <= 17) setPatientAge('adolescent');
    else if (age <= 64) setPatientAge('adult');
    else setPatientAge('senior');
  }, [patientDob]);

  useEffect(() => {
    if (!showAddCode || searchQuery.length < 2) {
      setSearchResults([]);
      return;
    }
    const timer = setTimeout(async () => {
      setSearchLoading(true);
      try {
        const results = await invoke<OhipCodeSearchResult[]>('search_ohip_codes', { query: searchQuery });
        setSearchResults(results);
      } catch (e) {
        console.error('Code search failed:', e);
      } finally {
        setSearchLoading(false);
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [showAddCode, searchQuery]);

  // Diagnostic code search with debounce
  useEffect(() => {
    if (!showDxSearch || dxQuery.length < 2) {
      setDxResults([]);
      return;
    }
    const timer = setTimeout(async () => {
      setDxSearchLoading(true);
      try {
        const results = await invoke<DiagnosticCodeSearchResult[]>('search_diagnostic_codes', { query: dxQuery });
        setDxResults(results);
      } catch (e) {
        console.error('Diagnostic code search failed:', e);
      } finally {
        setDxSearchLoading(false);
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [showDxSearch, dxQuery]);

  const handleSelectDiagnosticCode = useCallback((result: DiagnosticCodeSearchResult) => {
    if (!record) return;
    const updated = { ...record, diagnosticCode: result.code, diagnosticDescription: result.description };
    onRecordChange(updated);
    invoke('save_session_billing', { sessionId, date, record: updated }).catch(console.error);
    setShowDxSearch(false);
    setDxQuery('');
    setDxResults([]);
  }, [record, sessionId, date, onRecordChange]);

  const existingCodeIds = record?.codes.map(c => c.code) || [];
  const conflictMap = record ? findAllConflicts(existingCodeIds) : new Map();

  // Recalculate totals accounting for quantity
  const recalcTotals = useCallback((updated: BillingRecord) => {
    let shadow = 0, oob = 0, time = 0;
    for (const c of updated.codes) {
      const qty = (c.quantity ?? 1);
      const ahPremium = c.afterHours ? c.afterHoursPremiumCents : 0;
      if (c.category === 'in_basket') { shadow += (c.billableAmountCents + ahPremium) * qty; }
      else { oob += (c.billableAmountCents + ahPremium) * qty; }
    }
    time = updated.timeEntries.reduce((sum, t) => sum + t.billableAmountCents, 0);
    updated.totalShadowCents = shadow;
    updated.totalOutOfBasketCents = oob;
    updated.totalTimeBasedCents = time;
    updated.totalAmountCents = shadow + oob + time;
    onRecordChange(updated);
    invoke('save_session_billing', { sessionId, date, record: updated }).catch(console.error);
  }, [sessionId, date, onRecordChange]);

  const handleQuantityChange = useCallback((index: number, delta: number) => {
    if (!record) return;
    const updated = { ...record, codes: record.codes.map((c, i) => {
      if (i !== index) return c;
      const newQty = Math.max(1, Math.min(10, (c.quantity ?? 1) + delta));
      return { ...c, quantity: newQty };
    })};
    recalcTotals(updated);
  }, [record, recalcTotals]);

  const buildContext = useCallback((): BillingContext => {
    return {
      visitSetting,
      patientAge,
      referralReceived,
      counsellingExhausted,
      afterHoursOverride: afterHoursMode === 'auto' ? null : afterHoursMode === 'yes',
      isHospital,
    };
  }, [visitSetting, patientAge, referralReceived, counsellingExhausted, afterHoursMode, isHospital]);

  const handleExtract = useCallback(async () => {
    setExtracting(true);
    setExtractError(null);
    try {
      const context = buildContext();
      const result = await invoke<BillingRecord>('extract_billing_codes', {
        sessionId, date, context,
      });
      onRecordChange(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setExtractError(`Extraction failed: ${msg}`);
      console.error('Billing extraction failed:', e);
    } finally {
      setExtracting(false);
    }
  }, [sessionId, date, onRecordChange, buildContext]);

  const handleConfirm = useCallback(async () => {
    try {
      const result = await invoke<BillingRecord>('confirm_session_billing', {
        sessionId, date,
      });
      onRecordChange(result);
    } catch (e) {
      console.error('Billing confirm failed:', e);
    }
  }, [sessionId, date, onRecordChange]);

  const handleRemoveCode = useCallback((index: number) => {
    if (!record) return;
    const updated = { ...record, codes: record.codes.filter((_, i) => i !== index) };
    recalcTotals(updated);
  }, [record, recalcTotals]);

  const handleCopy = useCallback(async () => {
    if (!record) return;
    const lines = record.codes.map(c =>
      `${c.code} - ${c.description}: ${formatCents(c.billableAmountCents)}`
    );
    record.timeEntries.forEach(t => {
      lines.push(`${t.code} - ${t.description}: ${t.minutes}min = ${formatCents(t.billableAmountCents)}`);
    });
    lines.push(`Total: ${formatCents(record.totalAmountCents)}`);
    await writeText(lines.join('\n'));
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [record]);

  const handleAddCode = useCallback((result: OhipCodeSearchResult) => {
    if (!record) return;
    // Check if already added
    if (record.codes.some(c => c.code === result.code)) return;

    const newCode: BillingCode = {
      code: result.code,
      description: result.description,
      feeCents: result.feeCents,
      category: result.basket as BillingCategory,
      shadowPct: result.shadowPct,
      billableAmountCents: result.basket === 'in_basket'
        ? Math.round(result.feeCents * result.shadowPct / 100)
        : result.feeCents,
      confidence: 'high' as const,
      autoExtracted: false,
      afterHours: false,
      afterHoursPremiumCents: 0,
    };

    const updated = { ...record, codes: [...record.codes, newCode] };
    recalcTotals(updated);

    onRecordChange(updated);
    invoke('save_session_billing', { sessionId, date, record: updated }).catch(console.error);
    setShowAddCode(false);
    setSearchQuery('');
    setSearchResults([]);
  }, [record, sessionId, date, onRecordChange]);

  if (loading) {
    return <div className="billing-panel"><div className="loading-text">Loading billing data...</div></div>;
  }

  if (!record) {
    return (
      <div className="billing-panel billing-empty">
        <p>No billing codes extracted for this encounter.</p>

        {/* Collapsible Billing Context */}
        <div className="billing-context-section">
          <button
            className="billing-context-toggle"
            onClick={() => setContextExpanded(prev => !prev)}
          >
            <span className="billing-context-arrow">{contextExpanded ? '\u25BC' : '\u25B6'}</span>
            Billing Context
          </button>
          {contextExpanded && (
            <div className="billing-context-fields">
              <div className="billing-context-row">
                <label>Setting</label>
                <select value={visitSetting} onChange={e => setVisitSetting(e.target.value)}>
                  {VISIT_SETTING_OPTIONS.map(opt => (
                    <option key={opt.value} value={opt.value}>{opt.label}</option>
                  ))}
                </select>
              </div>
              <div className="billing-context-row">
                <label>Patient</label>
                <select value={patientAge} onChange={e => setPatientAge(e.target.value)}>
                  <option value="adult">Adult 18-64</option>
                  <option value="child_0_1">Child 0-1</option>
                  <option value="child_2_15">Child 2-15</option>
                  <option value="adolescent">Adolescent 16-17</option>
                  <option value="senior">Senior 65+</option>
                  <option value="idd">Adult with IDD</option>
                </select>
              </div>
              <div className="billing-context-row">
                <label>Referral</label>
                <label className="billing-context-checkbox">
                  <input
                    type="checkbox"
                    checked={referralReceived}
                    onChange={e => setReferralReceived(e.target.checked)}
                  />
                  Consultation
                </label>
              </div>
              <div className="billing-context-row">
                <label>K013</label>
                <select
                  value={counsellingExhausted ? 'exhausted' : 'available'}
                  onChange={e => setCounsellingExhausted(e.target.value === 'exhausted')}
                >
                  <option value="available">Available</option>
                  <option value="exhausted">3+ used (K033)</option>
                </select>
              </div>
              <div className="billing-context-row">
                <label>Hours</label>
                <select value={afterHoursMode} onChange={e => setAfterHoursMode(e.target.value as 'auto' | 'yes' | 'no')}>
                  <option value="auto">Auto-detect</option>
                  <option value="yes">After Hours</option>
                  <option value="no">Regular Hours</option>
                </select>
              </div>
              <div className="billing-context-row">
                <label>Location</label>
                <label className="billing-context-checkbox">
                  <input
                    type="checkbox"
                    checked={isHospital}
                    onChange={e => setIsHospital(e.target.checked)}
                  />
                  In Hospital (no tray fees)
                </label>
              </div>
            </div>
          )}
        </div>

        <button className="btn-generate" onClick={handleExtract} disabled={extracting}>
          {extracting ? 'Extracting...' : 'Extract Billing Codes'}
        </button>
        {extractError && (
          <div className="cap-warning-banner critical">{extractError}</div>
        )}
      </div>
    );
  }

  return (
    <div className="billing-panel">
      {/* Status bar */}
      <div className="billing-status-bar">
        <span className={`billing-status-badge ${record.status}`}>
          {record.status === 'confirmed' ? `Confirmed ${record.confirmedAt ? new Date(record.confirmedAt).toLocaleDateString() : ''}` : 'Draft'}
        </span>
        <div className="billing-actions">
          {record.status === 'draft' && (
            <button className="btn-small btn-confirm" onClick={handleConfirm}>Confirm</button>
          )}
          <button className="btn-small" onClick={handleCopy}>
            {copied ? '✓ Copied' : 'Copy'}
          </button>
          <button className="btn-small" onClick={handleExtract} disabled={extracting}>
            {extracting ? 'Extracting...' : 'Re-extract'}
          </button>
        </div>
      </div>

      {/* Diagnostic code */}
      <div className="insight-card billing-dx-card">
        <div className="insight-card-title">Diagnostic Code</div>
        {record.diagnosticCode ? (
          <div className="billing-dx-display">
            <span className="billing-dx-code">{record.diagnosticCode}</span>
            <span className="billing-dx-desc">{record.diagnosticDescription || ''}</span>
            {record.status === 'draft' && (
              <button className="btn-small" onClick={() => setShowDxSearch(true)}>Change</button>
            )}
          </div>
        ) : (
          <div className="billing-dx-display">
            <span className="billing-dx-missing">No diagnostic code set</span>
            <button className="btn-small" onClick={() => setShowDxSearch(true)}>Set Code</button>
          </div>
        )}
        {showDxSearch && (
          <div className="billing-search-container billing-dx-search">
            <input
              className="billing-search-input"
              type="text"
              placeholder="Search by code or diagnosis..."
              value={dxQuery}
              onChange={e => setDxQuery(e.target.value)}
              autoFocus
            />
            <button className="btn-small" onClick={() => { setShowDxSearch(false); setDxQuery(''); setDxResults([]); }}>Cancel</button>
            {dxResults.length > 0 && (
              <div className="billing-search-dropdown">
                {dxResults.map(result => (
                  <div
                    key={result.code}
                    className="billing-search-result"
                    onClick={() => handleSelectDiagnosticCode(result)}
                  >
                    <span className="billing-search-code">{result.code}</span>
                    <span className="billing-search-desc">{result.description}</span>
                    <span className="billing-search-category">{result.category}</span>
                  </div>
                ))}
              </div>
            )}
            {dxSearchLoading && <div className="billing-search-loading">Searching...</div>}
            {dxQuery.length >= 2 && !dxSearchLoading && dxResults.length === 0 && (
              <div className="billing-search-empty">No diagnostic codes found</div>
            )}
          </div>
        )}
      </div>

      {/* Billing codes table */}
      <div className="insight-card">
        <div className="insight-card-title">OHIP Billing Codes</div>
        {record.codes.length > 0 ? (
          <table className="billing-code-table">
            <thead>
              <tr>
                <th>Code</th>
                <th>Description</th>
                <th>Fee</th>
                <th>Rate</th>
                <th>Amount</th>
                <th>Qty</th>
                <th>Confidence</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {record.codes.map((code, i) => {
                const codeConflicts = conflictMap.get(code.code);
                return (
                  <tr key={`${code.code}-${i}`} className={`billing-code-row ${codeConflicts ? 'conflicted' : ''}`}>
                    <td className="billing-code-id" title={OHIP_CODE_CRITERIA[code.code] || code.description}>
                      {code.code}
                      {codeConflicts && <span className="billing-conflict-icon" title={codeConflicts.map((c: { code: string; reason: string }) => `Conflicts with ${c.code}: ${c.reason}`).join('\n')}>&#9888;</span>}
                    </td>
                    <td title={OHIP_CODE_CRITERIA[code.code] || ''}>{code.description}</td>
                    <td>{formatCents(code.feeCents)}</td>
                    <td>{code.category === 'in_basket' ? `${code.shadowPct}%` : '100%'}</td>
                    <td className="billing-amount">
                      {formatCents((code.billableAmountCents + (code.afterHours ? code.afterHoursPremiumCents : 0)) * (code.quantity ?? 1))}
                      {code.afterHours && <span className="billing-after-hours" title="After-hours premium">AH</span>}
                    </td>
                    <td className="billing-qty">
                      {record.status === 'draft' ? (
                        <span className="qty-controls">
                          <button className="qty-btn" onClick={() => handleQuantityChange(i, -1)} disabled={(code.quantity ?? 1) <= 1}>-</button>
                          <span className="qty-value">{code.quantity ?? 1}</span>
                          <button className="qty-btn" onClick={() => handleQuantityChange(i, 1)} disabled={(code.quantity ?? 1) >= 10}>+</button>
                        </span>
                      ) : (
                        <span>{code.quantity ?? 1}</span>
                      )}
                    </td>
                    <td><span className={`billing-confidence ${confidenceBadgeClass(code.confidence)}`}>{code.confidence}</span></td>
                    <td>
                      {record.status === 'draft' && (
                        <button className="btn-icon btn-remove" onClick={() => handleRemoveCode(i)} title="Remove">&times;</button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ) : (
          <p className="billing-empty-text">No billing codes extracted.</p>
        )}
        {record.status === 'draft' && (
          <div className="billing-add-code">
            {!showAddCode ? (
              <button className="btn-small" onClick={() => setShowAddCode(true)}>+ Add Code</button>
            ) : (
              <div className="billing-search-container">
                <input
                  className="billing-search-input"
                  type="text"
                  placeholder="Search by code or description..."
                  value={searchQuery}
                  onChange={e => setSearchQuery(e.target.value)}
                  autoFocus
                />
                <button className="btn-small" onClick={() => { setShowAddCode(false); setSearchQuery(''); setSearchResults([]); }}>Cancel</button>
                {searchResults.length > 0 && (
                  <div className="billing-search-dropdown">
                    {searchResults.map(result => {
                      const conflicts = findConflicts(existingCodeIds, result.code);
                      const alreadyAdded = existingCodeIds.includes(result.code);
                      return (
                        <div
                          key={result.code}
                          className={`billing-search-result ${conflicts.length > 0 ? 'conflicted' : ''} ${alreadyAdded ? 'disabled' : ''}`}
                          onClick={() => !alreadyAdded && handleAddCode(result)}
                          title={conflicts.length > 0 ? conflicts.map(c => `Conflicts with ${c.code}: ${c.reason}`).join('\n') : OHIP_CODE_CRITERIA[result.code] || ''}
                        >
                          <span className="billing-search-code">{result.code}</span>
                          <span className="billing-search-desc">{result.description}</span>
                          <span className="billing-search-fee">{formatCents(result.feeCents)}</span>
                          {conflicts.length > 0 && <span className="billing-conflict-badge">conflicts with {conflicts.map(c => c.code).join(', ')}</span>}
                          {alreadyAdded && <span className="billing-conflict-badge">already added</span>}
                        </div>
                      );
                    })}
                  </div>
                )}
                {searchLoading && <div className="billing-search-loading">Searching...</div>}
                {searchQuery.length >= 2 && !searchLoading && searchResults.length === 0 && (
                  <div className="billing-search-empty">No codes found</div>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Time entries */}
      <div className="insight-card">
        <div className="insight-card-title">Time-Based Billing (Q310-Q313)</div>
        <table className="billing-code-table">
          <thead>
            <tr>
              <th>Code</th>
              <th>Description</th>
              <th>Minutes</th>
              <th>Units</th>
              <th>Amount</th>
            </tr>
          </thead>
          <tbody>
            {record.timeEntries.map((te) => (
              <tr key={te.code} className="billing-code-row">
                <td className="billing-code-id" title={OHIP_CODE_CRITERIA[te.code] || te.description}>{te.code}</td>
                <td>{te.description}{te.autoCalculated ? ' (auto)' : ''}</td>
                <td>{te.minutes}</td>
                <td>{te.billableUnits}</td>
                <td className="billing-amount">{formatCents(te.billableAmountCents)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Totals */}
      <div className="insight-card billing-totals-card">
        <div className="insight-card-title">Encounter Total</div>
        <div className="billing-totals">
          <div className="billing-total-row">
            <span>Shadow billing (in-basket)</span>
            <span>{formatCents(record.totalShadowCents)}</span>
          </div>
          <div className="billing-total-row">
            <span>Out-of-basket (full FFS)</span>
            <span>{formatCents(record.totalOutOfBasketCents)}</span>
          </div>
          <div className="billing-total-row">
            <span>Time-based (Q310-Q313)</span>
            <span>{formatCents(record.totalTimeBasedCents)}</span>
          </div>
          <div className="billing-total-row billing-grand-total">
            <span>Total</span>
            <span>{formatCents(record.totalAmountCents)}</span>
          </div>
        </div>
      </div>
    </div>
  );
};
