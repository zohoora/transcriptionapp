#!/usr/bin/env python3
"""
mmWave SEN0395 presence sensor logger.
Logs all UART data with ISO 8601 timestamps (UTC) for correlation
with transcription app sessions.

Output: CSV file at ~/.transcriptionapp/mmwave/YYYY-MM-DD.csv

Logs both raw sensor readings and a 10-second software-debounced
presence state. Use presence_debounced for correlation with sessions.
"""

import serial
import time
import os
import sys
import signal
from datetime import datetime, timezone

SERIAL_PORT = '/dev/cu.usbserial-2110'
BAUD_RATE = 115200
LOG_DIR = os.path.expanduser('~/.transcriptionapp/mmwave')
DEBOUNCE_SECONDS = 10

def main():
    os.makedirs(LOG_DIR, exist_ok=True)

    today = datetime.now(timezone.utc).strftime('%Y-%m-%d')
    log_path = os.path.join(LOG_DIR, f'{today}.csv')

    write_header = not os.path.exists(log_path)

    ser = serial.Serial(SERIAL_PORT, BAUD_RATE, timeout=2)
    ser.reset_input_buffer()

    logfile = open(log_path, 'a', buffering=1)

    if write_header:
        logfile.write('timestamp_utc,timestamp_local,presence_raw,presence_debounced,raw\n')

    # Debounce state
    debounced = ''
    candidate = ''
    candidate_since = 0.0

    def shutdown(sig, frame):
        print(f'\nStopping logger. Log: {log_path}')
        logfile.close()
        ser.close()
        sys.exit(0)

    signal.signal(signal.SIGINT, shutdown)
    signal.signal(signal.SIGTERM, shutdown)

    print(f'mmWave logger started — {SERIAL_PORT} @ {BAUD_RATE}')
    print(f'Logging to: {log_path}')
    print(f'Debounce: {DEBOUNCE_SECONDS}s')
    print('Press Ctrl+C to stop.\n')

    while True:
        try:
            line = ser.readline().decode('ascii', errors='replace').strip()
            if not line:
                continue

            now_utc = datetime.now(timezone.utc)
            now_local = datetime.now().astimezone()
            now_ts = time.monotonic()
            ts_utc = now_utc.strftime('%Y-%m-%dT%H:%M:%S.%f')[:-3] + 'Z'
            ts_local = now_local.strftime('%Y-%m-%dT%H:%M:%S.%f')[:-3] + now_local.strftime('%z')

            # Parse presence from $JYBSS,<0|1>, , , *
            raw = ''
            if line.startswith('$JYBSS,'):
                parts = line.split(',')
                if len(parts) >= 2:
                    raw = parts[1]

            # Debounce: require new state to hold for DEBOUNCE_SECONDS
            if raw:
                if debounced == '':
                    debounced = raw
                    candidate = raw
                    candidate_since = now_ts
                elif raw != debounced:
                    if raw == candidate:
                        if now_ts - candidate_since >= DEBOUNCE_SECONDS:
                            debounced = raw
                            direction = "ARRIVED" if debounced == '1' else "LEFT"
                            print(f'  *** {ts_local} | {direction} ***')
                    else:
                        candidate = raw
                        candidate_since = now_ts
                else:
                    candidate = raw
                    candidate_since = now_ts

            raw_escaped = line.replace('"', '""')
            logfile.write(f'{ts_utc},{ts_local},{raw},{debounced},"{raw_escaped}"\n')

            status = 'PRESENT' if raw == '1' else 'ABSENT ' if raw == '0' else '???    '
            flicker = ' (flickering)' if raw != debounced else ''
            print(f'  {ts_local} | {status} | deb={debounced}{flicker}')

            # Rotate log file at midnight UTC
            new_today = now_utc.strftime('%Y-%m-%d')
            if new_today != today:
                today = new_today
                logfile.close()
                log_path = os.path.join(LOG_DIR, f'{today}.csv')
                logfile = open(log_path, 'a', buffering=1)
                logfile.write('timestamp_utc,timestamp_local,presence_raw,presence_debounced,raw\n')
                print(f'\n  --- Rotated to {log_path} ---\n')

        except serial.SerialException as e:
            print(f'Serial error: {e}. Reconnecting in 3s...')
            time.sleep(3)
            try:
                ser.close()
            except:
                pass
            ser = serial.Serial(SERIAL_PORT, BAUD_RATE, timeout=2)
            ser.reset_input_buffer()

if __name__ == '__main__':
    main()
