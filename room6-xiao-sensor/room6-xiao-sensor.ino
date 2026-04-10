/*
 * Room 6 XIAO ESP32-C3 mmWave Presence Sensor
 *
 * Reads mmWave radar via UART and outputs JSON over USB serial.
 * Auto-detects baud rate (tries 256000 for LD2410, then 115200 for SEN0395).
 * Dumps raw hex for diagnosis if no valid frames detected.
 *
 * Wiring: XIAO TX/RX pins 3(RX)/4(TX) → sensor UART
 */

#include <HardwareSerial.h>

HardwareSerial radarSerial(1);

// Pin assignments for XIAO ESP32-C3 with snap-on mmWave sensor
// Sensor TX → XIAO D2, Sensor RX → XIAO D3
// XIAO pin labels: D2=GPIO4, D3=GPIO5 (labels != GPIO numbers)
const int RX_PIN = 4;  // D2 = GPIO4 (XIAO receives sensor TX)
const int TX_PIN = 5;  // D3 = GPIO5 (XIAO transmits to sensor RX)

uint8_t buf[512];
int bufLen = 0;
int totalBytes = 0;
int frames = 0;
unsigned long lastReport = 0;
unsigned long bootTime = 0;
bool baudDetected = false;
int currentBaud = 0;

// LD2410 frame header/footer
const uint8_t HEADER[] = {0xF4, 0xF3, 0xF2, 0xF1};
const uint8_t FOOTER[] = {0xF8, 0xF7, 0xF6, 0xF5};

void tryBaud(int baud) {
  radarSerial.end();
  delay(100);
  radarSerial.begin(baud, SERIAL_8N1, RX_PIN, TX_PIN);
  currentBaud = baud;
  bufLen = 0;
  totalBytes = 0;
  frames = 0;
  Serial.print("{\"event\":\"trying_baud\",\"baud\":");
  Serial.print(baud);
  Serial.println("}");
  delay(500);
}

// ── LD2410 configuration commands ──────────────────────────────
// Command frame: FD FC FB FA [len_lo len_hi] [cmd data...] 04 03 02 01

void sendLD2410Command(const uint8_t* data, size_t len) {
  const uint8_t header[] = {0xFD, 0xFC, 0xFB, 0xFA};
  const uint8_t footer[] = {0x04, 0x03, 0x02, 0x01};
  radarSerial.write(header, 4);
  uint16_t dlen = len;
  radarSerial.write((uint8_t)(dlen & 0xFF));
  radarSerial.write((uint8_t)(dlen >> 8));
  radarSerial.write(data, len);
  radarSerial.write(footer, 4);
  radarSerial.flush();
  delay(100);
}

void enableConfig() {
  const uint8_t cmd[] = {0xFF, 0x00, 0x01, 0x00};
  sendLD2410Command(cmd, 4);
}

void endConfig() {
  const uint8_t cmd[] = {0xFE, 0x00};
  sendLD2410Command(cmd, 2);
}

// Set max moving gates, max stationary gates, and no-one timeout (seconds)
void setMaxGatesAndTimeout(uint8_t movingGates, uint8_t stationaryGates, uint16_t timeoutSecs) {
  uint8_t cmd[] = {
    0x60, 0x00,
    0x00, 0x00, movingGates, 0x00, 0x00, 0x00,
    0x01, 0x00, stationaryGates, 0x00, 0x00, 0x00,
    0x02, 0x00, (uint8_t)(timeoutSecs & 0xFF), (uint8_t)(timeoutSecs >> 8), 0x00, 0x00
  };
  sendLD2410Command(cmd, 20);
}

// Configure LD2410: max range 4 gates (3m), no-one timeout 1 second
void configureLD2410() {
  Serial.println("{\"event\":\"configuring_ld2410\"}");
  enableConfig();
  delay(100);
  setMaxGatesAndTimeout(4, 4, 1);  // 4 gates = 3m range, 1s timeout
  delay(100);
  endConfig();
  Serial.println("{\"event\":\"ld2410_configured\",\"max_gates\":4,\"range_m\":3,\"timeout_s\":1}");
}

void setup() {
  Serial.begin(115200);
  delay(1500);
  bootTime = millis();

  Serial.println("{\"event\":\"boot\",\"board\":\"xiao_esp32c3\",\"rx_pin\":3,\"tx_pin\":4}");

  // Try LD2410 baud first (256000)
  tryBaud(256000);
}

void dumpHex(int count) {
  // Dump first N bytes as hex for diagnosis
  int n = min(count, bufLen);
  Serial.print("{\"hex_dump\":\"");
  for (int i = 0; i < n; i++) {
    if (buf[i] < 0x10) Serial.print("0");
    Serial.print(buf[i], HEX);
    if (i < n - 1) Serial.print(" ");
  }
  Serial.print("\",\"len\":");
  Serial.print(n);
  Serial.println("}");
}

bool findLD2410Frame() {
  // Look for LD2410 header: F4 F3 F2 F1
  for (int i = 0; i <= bufLen - 4; i++) {
    if (buf[i] == HEADER[0] && buf[i+1] == HEADER[1] &&
        buf[i+2] == HEADER[2] && buf[i+3] == HEADER[3]) {
      frames++;

      // Parse target data if enough bytes
      if (i + 8 < bufLen) {
        uint16_t dlen = buf[i+4] | (buf[i+5] << 8);
        if (i + 6 + dlen <= bufLen && dlen >= 3) {
          uint8_t dtype = buf[i+6];
          uint8_t head = buf[i+7];
          uint8_t status = buf[i+8];

          if (dtype == 0x02 && head == 0xAA) {
            // Target data report
            Serial.print("{\"present\":");
            Serial.print(status != 0 ? "true" : "false");
            Serial.print(",\"status\":");
            Serial.print(status);

            // Moving target distance + energy
            if (dlen >= 7) {
              uint16_t moveDist = buf[i+9] | (buf[i+10] << 8);
              uint8_t moveEnergy = buf[i+11];
              Serial.print(",\"move_dist_cm\":");
              Serial.print(moveDist);
              Serial.print(",\"move_energy\":");
              Serial.print(moveEnergy);
            }
            // Stationary target distance + energy
            if (dlen >= 11) {
              uint16_t stillDist = buf[i+12] | (buf[i+13] << 8);
              uint8_t stillEnergy = buf[i+14];
              Serial.print(",\"still_dist_cm\":");
              Serial.print(stillDist);
              Serial.print(",\"still_energy\":");
              Serial.print(stillEnergy);
            }
            // Detection distance
            if (dlen >= 13) {
              uint16_t detDist = buf[i+15] | (buf[i+16] << 8);
              Serial.print(",\"det_dist_cm\":");
              Serial.print(detDist);
            }
            Serial.println("}");
          }
        }
      }

      // Consume processed data
      int skip = i + 4;
      memmove(buf, buf + skip, bufLen - skip);
      bufLen -= skip;
      return true;
    }
  }
  return false;
}

void loop() {
  // Read available bytes from radar
  while (radarSerial.available() && bufLen < 500) {
    buf[bufLen++] = radarSerial.read();
    totalBytes++;
  }

  // Try to find and parse frames
  findLD2410Frame();

  // Prevent buffer overflow
  if (bufLen > 450) {
    bufLen = 0;
  }

  // Auto-detect: if no frames after 5 seconds at current baud, try next
  if (!baudDetected && millis() - bootTime > 5000 && frames == 0 && currentBaud == 256000) {
    Serial.println("{\"event\":\"no_frames_at_256000\"}");
    if (totalBytes > 0) {
      dumpHex(32);
    }
    tryBaud(115200);
    bootTime = millis(); // Reset timer for new baud
  }

  if (!baudDetected && frames > 0) {
    baudDetected = true;
    Serial.print("{\"event\":\"baud_locked\",\"baud\":");
    Serial.print(currentBaud);
    Serial.println("}");
    configureLD2410();
  }

  // Status report every 2 seconds
  if (millis() - lastReport >= 2000) {
    lastReport = millis();
    Serial.print("{\"total_bytes\":");
    Serial.print(totalBytes);
    Serial.print(",\"frames\":");
    Serial.print(frames);
    Serial.print(",\"baud\":");
    Serial.print(currentBaud);
    Serial.print(",\"buf_len\":");
    Serial.print(bufLen);
    Serial.println("}");

    // Dump hex periodically if no frames detected (for debugging)
    if (frames == 0 && bufLen > 0) {
      dumpHex(32);
    }
  }
}
