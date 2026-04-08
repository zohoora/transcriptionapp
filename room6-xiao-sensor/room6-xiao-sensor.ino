#include <WiFi.h>
#include <HardwareSerial.h>
HardwareSerial radarSerial(1);
void setup() {
  WiFi.mode(WIFI_OFF);
  Serial.begin(115200);
  delay(1500);
  Serial.println("{\"event\":\"boot\"}");
  radarSerial.begin(256000, SERIAL_8N1, 3, 4);
  delay(500);
  Serial.println("{\"event\":\"uart_ok\"}");
}
int total = 0;
int frames = 0;
unsigned long lastReport = 0;
uint8_t buf[256];
int bufLen = 0;
void loop() {
  while (radarSerial.available() && bufLen < 256) {
    buf[bufLen++] = radarSerial.read();
    total++;
  }
  // Simple frame detection: count F4 F3 F2 F1 headers
  for (int i = 0; i <= bufLen - 4; i++) {
    if (buf[i]==0xF4 && buf[i+1]==0xF3 && buf[i+2]==0xF2 && buf[i+3]==0xF1) {
      frames++;
      // Try to read target status if enough data
      if (i + 8 < bufLen) {
        uint16_t dlen = buf[i+4] | (buf[i+5] << 8);
        if (i + 6 + dlen <= bufLen && dlen >= 3) {
          uint8_t dtype = buf[i+6];
          uint8_t head = buf[i+7];
          uint8_t status = buf[i+8];
          if (dtype == 0x02 && head == 0xAA) {
            Serial.print("{\"present\":");
            Serial.print(status != 0 ? "true" : "false");
            Serial.print(",\"status\":");
            Serial.print(status);
            if (dlen >= 13) {
              uint16_t dist = buf[i+15] | (buf[i+16] << 8);
              Serial.print(",\"dist_cm\":");
              Serial.print(dist);
            }
            Serial.println("}");
          }
        }
      }
      // Consume up to this point
      int skip = i + 4;
      memmove(buf, buf+skip, bufLen-skip);
      bufLen -= skip;
      break;
    }
  }
  if (bufLen > 200) bufLen = 0;
  if (millis() - lastReport >= 2000) {
    lastReport = millis();
    Serial.print("{\"total_bytes\":");
    Serial.print(total);
    Serial.print(",\"frames\":");
    Serial.print(frames);
    Serial.println("}");
  }
}
