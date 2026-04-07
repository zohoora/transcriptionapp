import React, { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { BillingRecord, BillingCode, OhipCodeSearchResult, BillingCategory } from '../../types';
import { formatCents, confidenceBadgeClass, OHIP_CODE_CRITERIA, findConflicts, findAllConflicts } from './billingUtils';

interface BillingTabProps {
  record: BillingRecord | null;
  loading: boolean;
  sessionId: string;
  date: string;
  durationMs: number | null;
  onRecordChange: (record: BillingRecord) => void;
}

export const BillingTab: React.FC<BillingTabProps> = ({
  record, loading, sessionId, date, onRecordChange,
}) => {
  const [extracting, setExtracting] = useState(false);
  const [copied, setCopied] = useState(false);
  const [extractError, setExtractError] = useState<string | null>(null);
  const [showAddCode, setShowAddCode] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<OhipCodeSearchResult[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);

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

  const handleExtract = useCallback(async () => {
    setExtracting(true);
    setExtractError(null);
    try {
      const result = await invoke<BillingRecord>('extract_billing_codes', {
        sessionId, date,
      });
      onRecordChange(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setExtractError(`Extraction failed: ${msg}`);
      console.error('Billing extraction failed:', e);
    } finally {
      setExtracting(false);
    }
  }, [sessionId, date, onRecordChange]);

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
