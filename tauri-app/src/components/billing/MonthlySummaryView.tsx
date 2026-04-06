import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { BillingMonthSummary } from '../../types';
import { formatCents } from './billingUtils';
import { CapProgressBar } from './CapProgressBar';

interface MonthlySummaryViewProps {
  endDate: string;
}

export const MonthlySummaryView: React.FC<MonthlySummaryViewProps> = ({ endDate }) => {
  const [summary, setSummary] = useState<BillingMonthSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    invoke<BillingMonthSummary>('get_monthly_billing_summary', { endDate })
      .then(s => { if (!cancelled) setSummary(s); })
      .catch(e => console.error('Failed to load monthly billing:', e))
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [endDate]);

  const handleExport = useCallback(async () => {
    if (!summary) return;
    setExporting(true);
    try {
      const csv = await invoke<string>('export_billing_csv', {
        startDate: summary.periodStart, endDate: summary.periodEnd,
      });
      await writeText(csv);
    } catch (e) {
      console.error('Export failed:', e);
    } finally {
      setExporting(false);
    }
  }, [summary]);

  if (loading) return <div className="billing-monthly-summary"><div className="loading-text">Loading monthly billing...</div></div>;
  if (!summary) return <div className="billing-monthly-summary"><p>No billing data.</p></div>;

  return (
    <div className="billing-monthly-summary">
      <div className="billing-summary-header">
        <h3>28-Day Rolling Summary</h3>
        <span className="billing-period">{summary.periodStart} to {summary.periodEnd}</span>
        <button className="btn-small" onClick={handleExport} disabled={exporting}>
          {exporting ? 'Copying...' : 'Export CSV'}
        </button>
      </div>

      {/* Revenue cards */}
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
          <div className="insight-card-title">Grand Total</div>
          <div className="billing-summary-value billing-grand-total">{formatCents(summary.totalAmountCents)}</div>
        </div>
      </div>

      {/* Cap progress bars */}
      <div className="insight-card">
        <div className="insight-card-title">FHO+ Caps (28-Day Window)</div>
        <CapProgressBar
          label="Total Hours"
          current={summary.capStatus.hoursUsed}
          limit={summary.capStatus.hoursLimit}
          warningLevel={summary.capStatus.warningLevel}
        />
        <CapProgressBar
          label="Indirect + Admin Ratio"
          current={summary.capStatus.indirectAdminRatio * 100}
          limit={summary.capStatus.indirectAdminLimit * 100}
          unit="%"
          warningLevel={summary.capStatus.warningLevel}
        />
        <CapProgressBar
          label="Admin-Only Ratio"
          current={summary.capStatus.adminRatio * 100}
          limit={summary.capStatus.adminLimit * 100}
          unit="%"
          warningLevel={summary.capStatus.warningLevel}
        />
        {summary.capStatus.projectedCapHitDate && (
          <div className="billing-projection">
            At current rate, 240hr cap projected for {summary.capStatus.projectedCapHitDate}
          </div>
        )}
      </div>

      {/* Daily totals table */}
      <div className="insight-card">
        <div className="insight-card-title">Daily Breakdown</div>
        <table className="billing-code-table">
          <thead>
            <tr>
              <th>Date</th>
              <th>Encounters</th>
              <th>Hours</th>
              <th>Shadow</th>
              <th>OOB</th>
              <th>Time</th>
              <th>Total</th>
            </tr>
          </thead>
          <tbody>
            {summary.dailySummaries.filter(d => d.encounterCount > 0).map((day) => (
              <tr key={day.date}>
                <td>{day.date}</td>
                <td>{day.encounterCount}</td>
                <td>{day.totalTimeHours.toFixed(1)}</td>
                <td>{formatCents(day.totalShadowCents)}</td>
                <td>{formatCents(day.totalOutOfBasketCents)}</td>
                <td>{formatCents(day.totalTimeBasedCents)}</td>
                <td className="billing-amount">{formatCents(day.totalAmountCents)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};
