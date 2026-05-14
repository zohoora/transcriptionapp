import { memo } from 'react';

/**
 * Placeholder slot for an allergies / problem list. No data wiring yet —
 * leaves the visual slot in place so future pipeline work has a home and
 * the sidebar's anatomy is stable across releases.
 */
export const AllergiesBlock = memo(function AllergiesBlock() {
  return (
    <section className="ca-sidebar-section">
      <h2>Allergies / Problems</h2>
      <div className="ca-allergies-empty">Not yet extracted — coming soon.</div>
    </section>
  );
});

export default AllergiesBlock;
