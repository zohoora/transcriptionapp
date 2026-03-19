import { memo } from 'react';

interface ActivePhysicianBadgeProps {
  physicianName: string;
  onSwitch: () => void;
}

/**
 * Small badge showing the active physician name with a "Switch" button.
 * Displayed in the header when a physician is selected.
 */
export const ActivePhysicianBadge = memo(function ActivePhysicianBadge({
  physicianName,
  onSwitch,
}: ActivePhysicianBadgeProps) {
  return (
    <div className="physician-badge">
      <span className="physician-badge-name">{physicianName}</span>
      <button
        className="physician-badge-switch"
        onClick={onSwitch}
        aria-label="Switch physician"
      >
        Switch
      </button>
    </div>
  );
});

export default ActivePhysicianBadge;
