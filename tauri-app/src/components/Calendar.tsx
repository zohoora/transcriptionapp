import React, { useMemo } from 'react';
import { isSameLocalDay, isToday } from '../utils';

interface CalendarProps {
  selectedDate: Date;
  onDateSelect: (date: Date) => void;
  datesWithSessions?: string[]; // YYYY-MM-DD format
}

const WEEKDAYS = ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'];
const MONTHS = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December'
];

const COLLAPSE_STORAGE_KEY = 'ami.historyCalendar.collapsed';

// formatDateKey uses local timezone for display/comparison with datesWithSessions
function formatDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

const Calendar: React.FC<CalendarProps> = ({
  selectedDate,
  onDateSelect,
  datesWithSessions = [],
}) => {
  const [viewDate, setViewDate] = React.useState(
    new Date(selectedDate.getFullYear(), selectedDate.getMonth(), 1)
  );
  const [collapsed, setCollapsed] = React.useState<boolean>(() => {
    try {
      return localStorage.getItem(COLLAPSE_STORAGE_KEY) === '1';
    } catch {
      return false;
    }
  });

  React.useEffect(() => {
    try {
      localStorage.setItem(COLLAPSE_STORAGE_KEY, collapsed ? '1' : '0');
    } catch {
      // localStorage unavailable (private mode, quota) — keep in-memory only
    }
  }, [collapsed]);

  // Bail-out via functional updater (returning prev) is load-bearing: it
  // preserves expanded month-browsing (March view while Feb 28 still
  // selected) instead of yanking the grid back when selectedDate changes.
  React.useEffect(() => {
    setViewDate(prev => {
      if (
        prev.getFullYear() === selectedDate.getFullYear() &&
        prev.getMonth() === selectedDate.getMonth()
      ) {
        return prev;
      }
      return new Date(selectedDate.getFullYear(), selectedDate.getMonth(), 1);
    });
  }, [selectedDate]);

  const sessionSet = useMemo(() => new Set(datesWithSessions), [datesWithSessions]);

  const { days, firstDayOfWeek } = useMemo(() => {
    const year = viewDate.getFullYear();
    const month = viewDate.getMonth();
    const firstDay = new Date(year, month, 1);
    const lastDay = new Date(year, month + 1, 0);
    const daysInMonth = lastDay.getDate();

    const days: Date[] = [];
    for (let i = 1; i <= daysInMonth; i++) {
      days.push(new Date(year, month, i));
    }

    return {
      days,
      firstDayOfWeek: firstDay.getDay(),
    };
  }, [viewDate]);

  const selectedLabel = useMemo(
    () =>
      selectedDate.toLocaleDateString(undefined, {
        weekday: 'short',
        month: 'short',
        day: 'numeric',
        year: 'numeric',
      }),
    [selectedDate],
  );

  const goToPrevMonth = () => {
    setViewDate(new Date(viewDate.getFullYear(), viewDate.getMonth() - 1, 1));
  };

  const goToNextMonth = () => {
    setViewDate(new Date(viewDate.getFullYear(), viewDate.getMonth() + 1, 1));
  };

  const goToPrevDay = () => {
    const d = new Date(selectedDate);
    d.setDate(d.getDate() - 1);
    onDateSelect(d);
  };

  const goToNextDay = () => {
    const d = new Date(selectedDate);
    d.setDate(d.getDate() + 1);
    onDateSelect(d);
  };

  const handleDateClick = (date: Date) => {
    onDateSelect(date);
  };

  const headerMode = collapsed
    ? {
        label: selectedLabel,
        prevLabel: 'Previous day',
        nextLabel: 'Next day',
        onPrev: goToPrevDay,
        onNext: goToNextDay,
      }
    : {
        label: `${MONTHS[viewDate.getMonth()]} ${viewDate.getFullYear()}`,
        prevLabel: 'Previous month',
        nextLabel: 'Next month',
        onPrev: goToPrevMonth,
        onNext: goToNextMonth,
      };

  return (
    <div className={`calendar${collapsed ? ' collapsed' : ''}`}>
      <div className="calendar-header">
        <button
          type="button"
          className="calendar-collapse-btn"
          onClick={() => setCollapsed(prev => !prev)}
          aria-label={collapsed ? 'Expand calendar' : 'Collapse calendar'}
          aria-expanded={!collapsed}
        >
          {collapsed ? '▸' : '▾'}
        </button>
        <span className="calendar-month-year">{headerMode.label}</span>
        <div className="calendar-nav-group">
          <button
            type="button"
            className="calendar-nav-btn"
            onClick={headerMode.onPrev}
            aria-label={headerMode.prevLabel}
          >
            &#9664;
          </button>
          <button
            type="button"
            className="calendar-nav-btn"
            onClick={headerMode.onNext}
            aria-label={headerMode.nextLabel}
          >
            &#9654;
          </button>
        </div>
      </div>

      {!collapsed && (
        <>
          <div className="calendar-weekdays">
            {WEEKDAYS.map((day) => (
              <div key={day} className="calendar-weekday">
                {day}
              </div>
            ))}
          </div>

          <div className="calendar-days">
            {Array.from({ length: firstDayOfWeek }).map((_, i) => (
              <div key={`empty-${i}`} className="calendar-day empty" />
            ))}

            {days.map((date) => {
              const dateKey = formatDateKey(date);
              const isSelected = isSameLocalDay(date, selectedDate);
              const hasSession = sessionSet.has(dateKey);
              const isTodayDate = isToday(date);

              return (
                <button
                  key={dateKey}
                  type="button"
                  className={`calendar-day ${isSelected ? 'selected' : ''} ${hasSession ? 'has-session' : ''} ${isTodayDate ? 'today' : ''}`}
                  onClick={() => handleDateClick(date)}
                  aria-label={`${MONTHS[date.getMonth()]} ${date.getDate()}, ${date.getFullYear()}${hasSession ? ', has sessions' : ''}`}
                >
                  {date.getDate()}
                  {hasSession && <span className="session-dot" />}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
};

export default Calendar;
