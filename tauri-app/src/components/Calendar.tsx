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

  const goToPrevMonth = () => {
    setViewDate(new Date(viewDate.getFullYear(), viewDate.getMonth() - 1, 1));
  };

  const goToNextMonth = () => {
    setViewDate(new Date(viewDate.getFullYear(), viewDate.getMonth() + 1, 1));
  };

  const handleDateClick = (date: Date) => {
    onDateSelect(date);
  };

  return (
    <div className="calendar">
      <div className="calendar-header">
        <button
          className="calendar-nav-btn"
          onClick={goToPrevMonth}
          aria-label="Previous month"
        >
          &#9664;
        </button>
        <span className="calendar-month-year">
          {MONTHS[viewDate.getMonth()]} {viewDate.getFullYear()}
        </span>
        <button
          className="calendar-nav-btn"
          onClick={goToNextMonth}
          aria-label="Next month"
        >
          &#9654;
        </button>
      </div>

      <div className="calendar-weekdays">
        {WEEKDAYS.map((day) => (
          <div key={day} className="calendar-weekday">
            {day}
          </div>
        ))}
      </div>

      <div className="calendar-days">
        {/* Empty cells for days before the first day of the month */}
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
    </div>
  );
};

export default Calendar;
