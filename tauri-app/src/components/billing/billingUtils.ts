import type { BillingConfidence, BillingStatus, CapWarningLevel } from '../../types';

export function formatCents(cents: number): string {
  return `$${(cents / 100).toFixed(2)}`;
}

export function formatHours(hours: number): string {
  return `${hours.toFixed(1)}h`;
}

export function capWarningColor(level: CapWarningLevel): string {
  switch (level) {
    case 'normal': return 'var(--accent-blue, #3b82f6)';
    case 'warning': return 'var(--accent-stopping, #f59e0b)';
    case 'critical':
    case 'exceeded': return 'var(--accent-recording, #ef4444)';
  }
}

export function confidenceBadgeClass(confidence: BillingConfidence): string {
  switch (confidence) {
    case 'high': return 'billing-confidence-high';
    case 'medium': return 'billing-confidence-medium';
    case 'low': return 'billing-confidence-low';
    default: return '';
  }
}

export function statusLabel(status: BillingStatus): string {
  return status === 'confirmed' ? 'Confirmed' : 'Draft';
}
