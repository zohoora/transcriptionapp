/*
 * ESP32 Presence Sensor Bridge
 *
 * Reads DFRobot SEN0395 mmWave sensor on Serial2 (GPIO14 RX, GPIO15 TX)
 * and exposes presence state over WiFi via HTTP.
 *
 * Endpoints:
 *   GET /          → JSON: {"present": true/false, "uptime_s": 123, "wifi_rssi": -65}
 *   GET /raw       → last raw NMEA sentence from sensor
 *   GET /health    → "ok"
 *   GET /cmd?q=... → send AT command to sensor, return response
 *
 * Also outputs status on USB serial (115200) for debugging.
 */

#include <WiFi.h>
#include <WebServer.h>

// ── WiFi Config ──────────────────────────────────────────────────────────────
const char* WIFI_SSID     = "AMER";
const char* WIFI_PASSWORD = "AMER1234AMER";

// ── Sensor UART Config ───────────────────────────────────────────────────────
// SEN0395: TX → ESP32 GPIO14 (RX2), RX → ESP32 GPIO15 (TX2)
#define SENSOR_RX_PIN 14
#define SENSOR_TX_PIN 15
#define SENSOR_BAUD   115200

// ── State ────────────────────────────────────────────────────────────────────
WebServer server(80);
bool sensorPresent = false;
unsigned long lastSensorUpdate = 0;
String lastRawSentence = "";
unsigned long wifiReconnectTimer = 0;
const unsigned long WIFI_RECONNECT_INTERVAL = 10000; // 10s between reconnect attempts

// ── LED feedback ─────────────────────────────────────────────────────────────
// Adafruit Feather V2 has onboard NeoPixel on GPIO0 but simple LED on GPIO13
#define LED_PIN 13

void setup() {
  // USB serial for debug
  Serial.begin(115200);
  delay(1000);
  Serial.println("\n=== ESP32 Presence Sensor Bridge ===");

  // LED
  pinMode(LED_PIN, OUTPUT);
  digitalWrite(LED_PIN, LOW);

  // Sensor UART
  Serial2.begin(SENSOR_BAUD, SERIAL_8N1, SENSOR_RX_PIN, SENSOR_TX_PIN);
  Serial.printf("Sensor UART: RX=%d TX=%d @ %d\n", SENSOR_RX_PIN, SENSOR_TX_PIN, SENSOR_BAUD);

  // WiFi
  connectWiFi();

  // HTTP routes
  server.on("/", handleRoot);
  server.on("/raw", handleRaw);
  server.on("/health", handleHealth);
  server.on("/cmd", handleCmd);
  server.onNotFound(handleNotFound);
  server.begin();
  Serial.println("HTTP server started on port 80");
}

void loop() {
  // 1. Read sensor data
  readSensor();

  // 2. Handle HTTP clients
  server.handleClient();

  // 3. Reconnect WiFi if needed
  if (WiFi.status() != WL_CONNECTED) {
    unsigned long now = millis();
    if (now - wifiReconnectTimer > WIFI_RECONNECT_INTERVAL) {
      wifiReconnectTimer = now;
      Serial.println("WiFi disconnected, reconnecting...");
      connectWiFi();
    }
    // Blink LED when disconnected
    digitalWrite(LED_PIN, (millis() / 500) % 2);
  } else {
    // Solid LED when connected, off when idle
    digitalWrite(LED_PIN, sensorPresent ? HIGH : LOW);
  }

  delay(10); // yield
}

// ── WiFi ─────────────────────────────────────────────────────────────────────
void connectWiFi() {
  Serial.printf("Connecting to WiFi '%s'...\n", WIFI_SSID);
  WiFi.mode(WIFI_STA);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);

  // Wait up to 15 seconds
  int attempts = 0;
  while (WiFi.status() != WL_CONNECTED && attempts < 30) {
    delay(500);
    Serial.print(".");
    attempts++;
  }

  if (WiFi.status() == WL_CONNECTED) {
    Serial.printf("\nWiFi connected! IP: %s  RSSI: %d dBm\n",
                  WiFi.localIP().toString().c_str(), WiFi.RSSI());
  } else {
    Serial.println("\nWiFi connection failed — will retry in background");
  }
}

// ── Sensor Reading ───────────────────────────────────────────────────────────
void readSensor() {
  static String lineBuffer = "";

  while (Serial2.available()) {
    char c = Serial2.read();
    if (c == '\n' || c == '\r') {
      if (lineBuffer.length() > 0) {
        parseSensorLine(lineBuffer);
        lineBuffer = "";
      }
    } else {
      lineBuffer += c;
      // Safety: prevent buffer overflow from garbage
      if (lineBuffer.length() > 128) {
        lineBuffer = "";
      }
    }
  }
}

void parseSensorLine(const String& line) {
  lastRawSentence = line;
  lastSensorUpdate = millis();

  // SEN0395 outputs: $JYBSS,0,*  (absent) or $JYBSS,1,*  (present)
  if (line.startsWith("$JYBSS,")) {
    int commaIdx = line.indexOf(',');
    if (commaIdx >= 0 && commaIdx + 1 < (int)line.length()) {
      char val = line.charAt(commaIdx + 1);
      bool newState = (val == '1');
      if (newState != sensorPresent) {
        Serial.printf("Presence: %s → %s\n",
                       sensorPresent ? "PRESENT" : "ABSENT",
                       newState ? "PRESENT" : "ABSENT");
      }
      sensorPresent = newState;
    }
  }
  // Also handle the leapMMW prompt (ignore it)
}

// ── HTTP Handlers ────────────────────────────────────────────────────────────
void handleRoot() {
  unsigned long sensorAge = (lastSensorUpdate > 0) ? (millis() - lastSensorUpdate) : 0;
  bool sensorStale = (lastSensorUpdate == 0) || (sensorAge > 5000);

  String json = "{";
  json += "\"present\":" + String(sensorPresent ? "true" : "false") + ",";
  json += "\"sensor_stale\":" + String(sensorStale ? "true" : "false") + ",";
  json += "\"sensor_age_ms\":" + String(sensorAge) + ",";
  json += "\"uptime_s\":" + String(millis() / 1000) + ",";
  json += "\"wifi_rssi\":" + String(WiFi.RSSI()) + ",";
  json += "\"ip\":\"" + WiFi.localIP().toString() + "\"";
  json += "}";

  server.sendHeader("Access-Control-Allow-Origin", "*");
  server.send(200, "application/json", json);
}

void handleRaw() {
  server.sendHeader("Access-Control-Allow-Origin", "*");
  server.send(200, "text/plain", lastRawSentence);
}

void handleHealth() {
  server.sendHeader("Access-Control-Allow-Origin", "*");
  server.send(200, "text/plain", "ok");
}

// ── AT Command Passthrough ──────────────────────────────────────────────────
void handleCmd() {
  if (!server.hasArg("q")) {
    server.send(400, "text/plain", "missing ?q= parameter");
    return;
  }

  String cmd = server.arg("q");
  Serial.printf("CMD> %s\n", cmd.c_str());

  // Drain any pending sensor data so it doesn't mix with the response
  while (Serial2.available()) Serial2.read();

  // Send command to sensor
  Serial2.print(cmd);
  Serial2.print("\r\n");

  // Collect response lines
  // - 2s idle timeout (no new bytes)
  // - 3s hard max (prevents hang on commands that trigger continuous output)
  String response = "";
  unsigned long hardDeadline = millis() + 3000;
  unsigned long idleDeadline = millis() + 2000;
  while (millis() < hardDeadline && millis() < idleDeadline) {
    while (Serial2.available()) {
      char c = Serial2.read();
      response += c;
      idleDeadline = millis() + 2000; // reset idle on each byte
    }
    delay(10);
  }

  Serial.printf("RSP> %s\n", response.c_str());

  server.sendHeader("Access-Control-Allow-Origin", "*");
  server.send(200, "text/plain", response);
}

void handleNotFound() {
  server.send(404, "text/plain", "not found");
}
