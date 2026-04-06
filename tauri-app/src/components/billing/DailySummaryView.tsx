import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { BillingDaySummary } from '../../types';
import { formatCents } from './billingUtils';
import { CapProgressBar } from './CapProgressBar';

interface DailySummaryViewProps {
  date: string;
}

export const DailySummaryView: React.FC<DailySummaryViewProps> = ({ date }) => {
  const [summary, setSummary] = useState<BillingDaySummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    invoke<BillingDaySummary>('get_daily_billing_summary', { date })
      .then(s => { if (!cancelled) setSummary(s); })
      .catch(e => console.error('Failed to load daily billing:', e))
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [date]);

  const handleExport = useCallback(async () => {
    setExporting(true);
    try {
      const csv = await invoke<string>('export_billing_csv', { startDate: date, endDate: date });
      await writeText(csv);
    } catch (e) {
      console.error('Export failed:', e);
    } finally {
      setExporting(false);
    }
  }, [date]);

  if (loading) return <div className="billing-daily-summary"><div className="loading-text">Loading daily billing...</div></div>;
  if (!summary) return <div className="billing-daily-summary"><p>No billing data for this date.</p></div>;

  return (
    <div className="billing-daily-summary">
      <div className="billing-summary-header">
        <h3>Daily Billing Summary — {date}</h3>
        <button className="btn-small" onClick={handleExport} disabled={exporting}>
          {exporting ? 'Copying...' : 'Export CSV'}
        </button>
      </div>

      {/* Summary cards */}
      <div className="billing-summary-cards">
        <div className="insight-card">
          <div className="insight-card-title">Shadow Billing</div>
          <div className="billing-summary-value">{formatCents(summary.totalShadowCents)}</div>
        </div>
        <div className="insight-card">
          <div className="insight-card-title">Out-of-Basket</div>
          <div className="billing-summary-value">{formatCents(summary.totalOutOfBasketCents)}</div>
        </div>
        <div className="insight-card">
          <div className="insight-card-title">Time-Based</div>
          <div className="billing-summary-value">{formatCents(summary.totalTimeBasedCents)}</div>
        </div>
        <div className="insight-card">
          <div className="insight-card-title">Total</div>
          <div className="billing-summary-value billing-grand-total">{formatCents(summary.totalAmountCents)}</div>
        </div>
      </div>

      {/* Daily cap */}
      <div className="insight-card">
        <div className="insight-card-title">Daily Time Cap</div>
        <CapProgressBar
          label="Hours"
          current={summary.capStatus.hoursUsed}
          limit={summary.capStatus.hoursLimit}
          warningLevel={summary.capStatus.warningLevel}
        />
        <div className="billing-cap-details">
          <span>{summary.encounterCount} encounters</span>
          <span>{summary.confirmedCount} confirmed, {summary.draftCount} draft</span>
        </div>
      </div>

      {/* Per-encounter breakdown */}
      <div className="insight-card">
        <div className="insight-card-title">Encounters</div>
        <table className="billing-code-table">
          <thead>
            <tr>
              <th>Patient</th>
              <th>Codes</th>
              <th>Time</th>
              <th>Total</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            {summary.encounters.map((enc) => (
              <tr key={enc.sessionId}>
                <td>{enc.patientName || 'Unknown'}</td>
                <td>{enc.codes.length}</td>
                <td>{enc.timeEntries.reduce((s, t) => s + t.minutes, 0)}min</td>
                <td className="billing-amount">{formatCents(enc.totalAmountCents)}</td>
                <td><span className={`billing-status-badge ${enc.status}`}>{enc.status}</span></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};
