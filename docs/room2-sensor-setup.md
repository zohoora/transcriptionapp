# Room 2 Presence Sensor Setup

## What This Is

A DFRobot SEN0395 mmWave presence sensor is connected to an Adafruit FT232H USB-to-UART adapter via USB-C. The sensor has been pre-configured with clinic-appropriate settings (matching Room 6). It needs to be plugged into the iMac and the AMI Assist app configured to use it.

## Hardware

- **Sensor**: DFRobot SEN0395 (24GHz mmWave human presence detection)
- **Interface**: FT232H USB-to-UART breakout (USB-C)
- **Wiring**: Sensor RX → FT232H D0, Sensor TX → FT232H D1
- **Baud rate**: 115200
- **Protocol**: NMEA-style `$JYBSS,{0|1}` (0 = absent, 1 = present)

## Sensor Configuration (Already Done)

The sensor has been flashed with these settings (saved to flash, persists across power cycles):

| Parameter | Value | Notes |
|-----------|-------|-------|
| Range | 0 - 4.95m | Matches Room 6; appropriate for exam room size |
| Sensitivity | 7 (of 9) | Good balance of detection vs false positives |
| Output latency | 0.025s - 0.25s | Fast response time |

No further sensor configuration is needed. Just plug in and configure the app.

## Setup Steps

### 1. Plug in the FT232H

Connect the FT232H USB-C cable to the iMac. The sensor gets power from USB.

### 2. Find the serial port

Run:
```bash
ls /dev/cu.usbserial-*
```

You should see something like `/dev/cu.usbserial-XXXX` (e.g., `/dev/cu.usbserial-2120`). Note this path.

If nothing appears, also try:
```bash
ls /dev/cu.usb*
```

### 3. Verify the sensor is outputting data

```bash
python3 -c "
import serial, time
port = '/dev/cu.usbserial-XXXX'  # Replace with actual port from step 2
ser = serial.Serial(port, 115200, timeout=2)
for _ in range(5):
    line = ser.readline()
    if line:
        print(line.decode(errors='replace').strip())
ser.close()
"
```

You should see lines like `$JYBSS,0, , , *` (no one present) or `$JYBSS,1, , , *` (someone present). If you see this, the sensor is working.

If `pyserial` is not installed: `pip3 install pyserial`

### 4. Edit the app config

Edit `~/.transcriptionapp/config.json`. Set these fields (create them if they don't exist):

```json
{
  "presence_sensor_port": "/dev/cu.usbserial-XXXX",
  "presence_sensor_url": "",
  "encounter_detection_mode": "hybrid",
  "presence_absence_threshold_secs": 180,
  "presence_debounce_secs": 10,
  "presence_csv_log_enabled": true,
  "hybrid_confirm_window_secs": 180,
  "hybrid_min_words_for_sensor_split": 500
}
```

Key points:
- **`presence_sensor_port`**: Set to the exact serial port path from step 2
- **`presence_sensor_url`**: MUST be empty string `""` — this tells the app to use serial instead of HTTP/WiFi
- **`encounter_detection_mode`**: Set to `"hybrid"` — sensor provides early departure warning, LLM confirms the split

The other fields are defaults matching Room 6, but include them for completeness.

### 5. Restart AMI Assist

Close and reopen the app. When continuous mode starts, the activity log should show:
```
Serial sensor connected: /dev/cu.usbserial-XXXX
```

The sensor status will appear in the continuous mode dashboard.

## How It Works

In **hybrid** detection mode:
1. The mmWave sensor continuously reports presence/absence at ~1Hz via serial
2. When a patient leaves (Present → Absent), the sensor triggers an **accelerated** LLM check (~30s instead of the normal ~8 min timer)
3. The LLM evaluates the transcript to confirm if an encounter boundary occurred
4. If the sensor reports absence for 180s (`hybrid_confirm_window_secs`) and there are 500+ words, it force-splits even if the LLM disagrees
5. If the sensor regains presence before the LLM confirms, the split is cancelled

This is better than LLM-only (faster encounter splits when patients leave) or sensor-only (avoids false splits from brief hallway passes).

## Troubleshooting

| Problem | Fix |
|---------|-----|
| No `/dev/cu.usbserial-*` device | Check USB-C connection. Try a different port. Run `system_profiler SPUSBDataType` to see if FTDI device is detected |
| Serial port permission denied | Run `sudo chmod 666 /dev/cu.usbserial-XXXX` |
| "Device or resource busy" | Another process is using the port. Kill any `python3` or `screen` sessions accessing it |
| Sensor not detected by app | Make sure `presence_sensor_url` is empty (not just missing — explicitly set to `""`) |
| Sensor detected but never triggers splits | Check that `encounter_detection_mode` is `"hybrid"`, not `"llm"` |
| False splits when no one left | Sensor may be picking up hallway traffic. Consider reducing range (see AT commands below) |

## AT Commands (If You Need to Reconfigure)

To send commands to the sensor via the FT232H, use this Python snippet:

```python
import serial, time
ser = serial.Serial('/dev/cu.usbserial-XXXX', 115200, timeout=2)
time.sleep(0.3)

# Stop sensing before changing config
ser.write(b'sensorStop\r\n')
time.sleep(1)

# Example: change range to 0-3m (20 segments * 0.15m = 3.0m)
ser.write(b'detRangeCfg -1 0 20\r\n')
time.sleep(0.5)

# Read current config
for cmd in ['getRange', 'getSensitivity', 'getLatency']:
    ser.write(f'{cmd}\r\n'.encode())
    time.sleep(0.5)
    while ser.in_waiting:
        print(ser.read(ser.in_waiting).decode(errors='replace'), end='')

# Save config to flash (required! uses magic bytes)
ser.write(b'saveCfg 0x45670123 0xCDEF89AB 0x956128C6 0xDF54AC89\r\n')
time.sleep(1)

# Restart sensing
ser.write(b'sensorStart\r\n')
time.sleep(0.5)

ser.close()
```

### Command Reference

| Command | Unit | Example | Notes |
|---------|------|---------|-------|
| `detRangeCfg -1 <start> <end>` | 0.15m per segment | `detRangeCfg -1 0 33` = 0-4.95m | Max ~60 segments (9m) |
| `outputLatency -1 <delay> <hold>` | 25ms per unit | `outputLatency -1 1 10` = 25ms-250ms | |
| `setSensitivity <level>` | 0-9 | `setSensitivity 7` | 9 = most sensitive |
| `getRange` | — | Returns e.g. `Response 0.000 4.950` | |
| `getSensitivity` | — | Returns e.g. `Response 7` | |
| `getLatency` | — | Returns e.g. `Response 0.025 0.250` | |
| `sensorStop` | — | Stop sensing (required before config changes) | |
| `sensorStart` | — | Resume sensing (fails if unsaved changes exist) | |
| `saveCfg 0x45670123 0xCDEF89AB 0x956128C6 0xDF54AC89` | — | Save to flash (magic bytes required) | |

## Current Sensor Settings (For Reference)

These match Room 6 (production-validated):
- **Range**: 33 segments = 4.95m
- **Sensitivity**: 7
- **Latency**: 1-10 units = 25ms-250ms
