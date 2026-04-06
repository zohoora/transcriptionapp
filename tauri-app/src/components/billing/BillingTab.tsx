import React, { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { BillingRecord } from '../../types';
import { formatCents, confidenceBadgeClass, OHIP_CODE_CRITERIA } from './billingUtils';

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
    // Recalculate totals (must match Rust BillingRecord::recalculate_totals)
    let shadow = 0, oob = 0, time = 0;
    for (const c of updated.codes) {
      const amount = c.billableAmountCents;
      const ahPremium = c.afterHours ? c.afterHoursPremiumCents : 0;
      if (c.category === 'in_basket') {
        shadow += amount + ahPremium;
      } else {
        oob += amount + ahPremium;
      }
    }
    time = updated.timeEntries.reduce((sum, t) => sum + t.billableAmountCents, 0);
    updated.totalShadowCents = shadow;
    updated.totalOutOfBasketCents = oob;
    updated.totalTimeBasedCents = time;
    updated.totalAmountCents = shadow + oob + time;
    onRecordChange(updated);
    invoke('save_session_billing', { sessionId, date, record: updated }).catch(console.error);
  }, [record, sessionId, date, onRecordChange]);

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
                <th>Confidence</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {record.codes.map((code, i) => (
                <tr key={`${code.code}-${i}`} className="billing-code-row">
                  <td className="billing-code-id" title={OHIP_CODE_CRITERIA[code.code] || code.description}>{code.code}</td>
                  <td title={OHIP_CODE_CRITERIA[code.code] || ''}>{code.description}</td>
                  <td>{formatCents(code.feeCents)}</td>
                  <td>{code.category === 'in_basket' ? `${code.shadowPct}%` : '100%'}</td>
                  <td className="billing-amount">
                    {formatCents(code.billableAmountCents + code.afterHoursPremiumCents)}
                    {code.afterHours && <span className="billing-after-hours" title="After-hours premium">AH</span>}
                  </td>
                  <td><span className={`billing-confidence ${confidenceBadgeClass(code.confidence)}`}>{code.confidence}</span></td>
                  <td>
                    {record.status === 'draft' && (
                      <button className="btn-icon btn-remove" onClick={() => handleRemoveCode(i)} title="Remove">&times;</button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="billing-empty-text">No billing codes extracted.</p>
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
