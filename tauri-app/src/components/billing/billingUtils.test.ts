/**
 * Tests for billingUtils.ts — pure formatting + conflict detection helpers.
 */
import { describe, it, expect } from 'vitest';
import {
  formatCents,
  formatHours,
  capWarningColor,
  confidenceBadgeClass,
  statusLabel,
  findConflicts,
  findAllConflicts,
} from './billingUtils';

describe('formatCents', () => {
  it('formats whole dollars', () => {
    expect(formatCents(0)).toBe('$0.00');
    expect(formatCents(100)).toBe('$1.00');
    expect(formatCents(2000)).toBe('$20.00');
  });

  it('formats partial dollars', () => {
    expect(formatCents(99)).toBe('$0.99');
    expect(formatCents(4455)).toBe('$44.55');
  });

  it('handles negative amounts (sign in cents position)', () => {
    // Implementation: just `$${(cents/100).toFixed(2)}` — sign appears after $
    expect(formatCents(-100)).toBe('$-1.00');
  });
});

describe('formatHours', () => {
  it('formats whole hours', () => {
    expect(formatHours(1)).toBe('1.0h');
    expect(formatHours(8)).toBe('8.0h');
  });

  it('formats fractional hours', () => {
    expect(formatHours(0.5)).toBe('0.5h');
    expect(formatHours(1.25)).toBe('1.3h'); // rounded to 1 decimal
  });
});

describe('capWarningColor', () => {
  it('returns CSS variable colors per level', () => {
    // All return CSS var() expressions
    expect(capWarningColor('normal')).toContain('blue');
    expect(capWarningColor('warning')).toContain('stopping');
    expect(capWarningColor('critical')).toContain('recording');
    expect(capWarningColor('exceeded')).toContain('recording');
  });
});

describe('confidenceBadgeClass', () => {
  it('returns CSS class per confidence level', () => {
    expect(confidenceBadgeClass('high')).toContain('high');
    expect(confidenceBadgeClass('medium')).toContain('medium');
    expect(confidenceBadgeClass('low')).toContain('low');
  });
});

describe('statusLabel', () => {
  it('returns human-readable status labels', () => {
    expect(statusLabel('draft')).toBe('Draft');
    expect(statusLabel('confirmed')).toBe('Confirmed');
  });
});

describe('findConflicts', () => {
  it('returns no conflicts when none exist', () => {
    const conflicts = findConflicts(['A007A'], 'G370A');
    expect(conflicts).toEqual([]);
  });

  it('detects A001A vs A007A as mutually exclusive (assessment codes)', () => {
    // Two assessment codes for the same encounter is invalid
    const conflicts = findConflicts(['A001A'], 'A007A');
    expect(conflicts.length).toBeGreaterThan(0);
    expect(conflicts[0].code).toBe('A001A');
  });

  it('detects K013A vs A007A as mutually exclusive (K013 standalone rule)', () => {
    const conflicts = findConflicts(['A007A'], 'K013A');
    expect(conflicts.length).toBeGreaterThan(0);
  });

  it('does not flag base+addon pairs as conflicts (G370A + G371A)', () => {
    // G370A and G371A are a base+addon pair — both can be billed together
    const conflicts = findConflicts(['G370A'], 'G371A');
    expect(conflicts).toEqual([]);
  });
});

describe('findAllConflicts', () => {
  it('returns empty map for non-conflicting codes', () => {
    const map = findAllConflicts(['A007A', 'G370A', 'Q310A']);
    expect(map.size).toBe(0);
  });

  it('flags conflicting codes in both directions', () => {
    const map = findAllConflicts(['A001A', 'A007A']);
    // Both A001A and A007A should appear as keys with each other listed
    expect(map.size).toBe(2);
    expect(map.get('A001A')).toBeDefined();
    expect(map.get('A007A')).toBeDefined();
  });
});
