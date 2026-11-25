#include <Arduino.h>
#include <WiFi.h>
#include <WebServer.h>
#include <WiFiUdp.h>
#include <ESPmDNS.h>
#include <Preferences.h>
#include <Update.h>
#include <HTTPClient.h>
#include <NetworkClientSecure.h>
#include <FastLED.h>

#define MAX_LEDS 300
#define UDP_PORT 4210
#define DEFAULT_LED_PIN 18
#define BOOT_BUTTON_PIN 0
#define BOOT_RESET_HOLD_MS 3000

const char* FIRMWARE_VERSION = "1.0.0";
const char* MDNS_NAME = "Aurapro";
const char* AP_SSID = "Aura Pro";
const char* AP_PASS = "Aura1234";


const char* GITHUB_OWNER = "ganithpasindu";
const char* GITHUB_REPO  = "Aura-Pro";
const char* BIN_ASSET_NAME = "aura_pro_esp32.bin";

Preferences prefs;
WebServer server(80);
WiFiUDP udp;

CRGB leds[MAX_LEDS];
String savedSsid = "";
String savedPass = "";
int savedPin = DEFAULT_LED_PIN;
int numLeds = 60;
bool apModeActive = false;

unsigned long bootPressStartMs = 0;
bool bootResetTriggered = false;

// OTA Status & Progress state (protected by mutex)
portMUX_TYPE otaMux = portMUX_INITIALIZER_UNLOCKED;
volatile bool otaRunning = false;
volatile bool otaDone = false;
volatile float otaProgress = 0.0f;
volatile unsigned long otaStartMs = 0;
String otaState = "idle"; // "idle", "checking", "downloading", "flashing", "done", "error"
String otaMessage = "";
bool otaTriggerPending = false;

void setOtaStatus(const char* state, float progress, const String& msg, bool running, bool done) {
  portENTER_CRITICAL(&otaMux);
  otaState = state;
  otaProgress = progress;
  otaMessage = msg;
  otaRunning = running;
  otaDone = done;
  portEXIT_CRITICAL(&otaMux);
}

int parseVersionPart(const String& ver, int index) {
  int start = 0;
  int currentIdx = 0;
  while (start <= (int)ver.length()) {
    int dot = ver.indexOf('.', start);
    String part = (dot < 0) ? ver.substring(start) : ver.substring(start, dot);
    if (currentIdx == index) {
      return part.toInt();
    }
    if (dot < 0) break;
    start = dot + 1;
    currentIdx++;
  }
  return 0;
}

// returns >0 if remote > local, 0 if equal, <0 if remote < local
int compareSemver(const String& remoteRaw, const String& localRaw) {
  String r = remoteRaw;
  String l = localRaw;
  r.trim();
  l.trim();
  if (r.startsWith("v") || r.startsWith("V")) r.remove(0, 1);
  if (l.startsWith("v") || l.startsWith("V")) l.remove(0, 1);

  int rPlus = r.indexOf('+');
  if (rPlus >= 0) r = r.substring(0, rPlus);
  int rDash = r.indexOf('-');
  if (rDash >= 0) r = r.substring(0, rDash);

  int lPlus = l.indexOf('+');
  if (lPlus >= 0) l = l.substring(0, lPlus);
  int lDash = l.indexOf('-');
  if (lDash >= 0) l = l.substring(0, lDash);

  for (int i = 0; i < 3; i++) {
    int rv = parseVersionPart(r, i);
    int lv = parseVersionPart(l, i);
    if (rv > lv) return 1;
    if (rv < lv) return -1;
  }
  return 0;
}

String extractJsonStringField(const String& json, const String& key) {
  String pattern = "\"" + key + "\"";
  int keyPos = json.indexOf(pattern);
  if (keyPos < 0) return "";
  int colonPos = json.indexOf(":", keyPos + pattern.length());
  if (colonPos < 0) return "";
  int firstQuote = json.indexOf("\"", colonPos + 1);
  if (firstQuote < 0) return "";
  int secondQuote = json.indexOf("\"", firstQuote + 1);
  if (secondQuote < 0) return "";
  return json.substring(firstQuote + 1, secondQuote);
}

String jsonEscape(const String& s) {
  String out = "";
  out.reserve(s.length() + 4);
  for (size_t i = 0; i < s.length(); i++) {
    char c = s[i];
    if (c == '"' || c == '\\') {
      out += '\\';
      out += c;
    } else if (c >= 32) {
      out += c;
    }
  }
  return out;
}

void saveConfig(const String& ssid, const String& pass, int pin) {
  prefs.begin("ambilight", false);
  prefs.putString("ssid", ssid);
  prefs.putString("pass", pass);
  prefs.putInt("pin", pin);
  prefs.putBool("configured", true);
  prefs.end();
}

void clearConfigAndReboot() {
  prefs.begin("ambilight", false);
  prefs.clear();
  prefs.end();
  delay(200);
  ESP.restart();
}

void checkBootButtonReset() {
  if (digitalRead(BOOT_BUTTON_PIN) == LOW) {
    unsigned long now = millis();
    if (bootPressStartMs == 0) {
      bootPressStartMs = now;
    } else if (!bootResetTriggered && (now - bootPressStartMs >= BOOT_RESET_HOLD_MS)) {
      bootResetTriggered = true;
      for (int i = 0; i < MAX_LEDS; i++) {
        leds[i] = CRGB::Magenta;
      }
      FastLED.show();
      clearConfigAndReboot();
    }
  } else {
    bootPressStartMs = 0;
    bootResetTriggered = false;
  }
}

void attachLedStrip(int pin) {
  switch (pin) {
    case 2:  FastLED.addLeds<WS2812B, 2, GRB>(leds, MAX_LEDS); break;
    case 4:  FastLED.addLeds<WS2812B, 4, GRB>(leds, MAX_LEDS); break;
    case 5:  FastLED.addLeds<WS2812B, 5, GRB>(leds, MAX_LEDS); break;
    case 13: FastLED.addLeds<WS2812B, 13, GRB>(leds, MAX_LEDS); break;
    case 14: FastLED.addLeds<WS2812B, 14, GRB>(leds, MAX_LEDS); break;
    case 15: FastLED.addLeds<WS2812B, 15, GRB>(leds, MAX_LEDS); break;
    case 16: FastLED.addLeds<WS2812B, 16, GRB>(leds, MAX_LEDS); break;
    case 17: FastLED.addLeds<WS2812B, 17, GRB>(leds, MAX_LEDS); break;
    case 18: FastLED.addLeds<WS2812B, 18, GRB>(leds, MAX_LEDS); break;
    case 19: FastLED.addLeds<WS2812B, 19, GRB>(leds, MAX_LEDS); break;
    case 21: FastLED.addLeds<WS2812B, 21, GRB>(leds, MAX_LEDS); break;
    case 22: FastLED.addLeds<WS2812B, 22, GRB>(leds, MAX_LEDS); break;
    case 23: FastLED.addLeds<WS2812B, 23, GRB>(leds, MAX_LEDS); break;
    case 25: FastLED.addLeds<WS2812B, 25, GRB>(leds, MAX_LEDS); break;
    default: FastLED.addLeds<WS2812B, 18, GRB>(leds, MAX_LEDS); break;
  }
  FastLED.clear(true);
}

void runGitHubOtaJob() {
  otaStartMs = millis();
  setOtaStatus("checking", 0.05f, "Checking GitHub release...", true, false);

  if (WiFi.status() != WL_CONNECTED) {
    setOtaStatus("error", 0.0f, "ESP32 Wi-Fi not connected", false, true);
    return;
  }

  NetworkClientSecure client;
  client.setInsecure();

  HTTPClient apiHttp;
  apiHttp.setFollowRedirects(HTTPC_FORCE_FOLLOW_REDIRECTS);
  apiHttp.addHeader("User-Agent", "aura-pro-esp32-ota");
  apiHttp.setTimeout(20000);

  String apiUrl = "https://api.github.com/repos/" + String(GITHUB_OWNER) + "/" +
                  String(GITHUB_REPO) + "/releases/latest";

  if (!apiHttp.begin(client, apiUrl)) {
    setOtaStatus("error", 0.0f, "Failed to connect GitHub API", false, true);
    return;
  }

  int code = apiHttp.GET();
  if (code != HTTP_CODE_OK) {
    String err = "GitHub API HTTP " + String(code);
    apiHttp.end();
    setOtaStatus("error", 0.0f, err, false, true);
    return;
  }

  String payload = apiHttp.getString();
  apiHttp.end();

  String tagName = extractJsonStringField(payload, "tag_name");
  if (tagName.length() == 0) {
    setOtaStatus("error", 0.0f, "Release tag_name not found", false, true);
    return;
  }

  if (compareSemver(tagName, String(FIRMWARE_VERSION)) <= 0) {
    setOtaStatus("done", 1.0f, "Already on latest firmware (v" + String(FIRMWARE_VERSION) + ")", false, true);
    return;
  }

  setOtaStatus("downloading", 0.12f, "Downloading " + String(BIN_ASSET_NAME) + " (" + tagName + ")...", true, false);

  String binUrl = "https://github.com/" + String(GITHUB_OWNER) + "/" +
                  String(GITHUB_REPO) + "/releases/download/" + tagName + "/" +
                  String(BIN_ASSET_NAME);

  HTTPClient binHttp;
  binHttp.setFollowRedirects(HTTPC_FORCE_FOLLOW_REDIRECTS);
  binHttp.addHeader("User-Agent", "aura-pro-esp32-ota");
  binHttp.setTimeout(90000);

  if (!binHttp.begin(client, binUrl)) {
    setOtaStatus("error", 0.0f, "Failed to start firmware download", false, true);
    return;
  }

  int binCode = binHttp.GET();
  if (binCode != HTTP_CODE_OK) {
    String err = "Firmware download HTTP " + String(binCode);
    binHttp.end();
    setOtaStatus("error", 0.0f, err, false, true);
    return;
  }

  int totalLen = binHttp.getSize();
  size_t beginLen = (totalLen > 0) ? (size_t)totalLen : UPDATE_SIZE_UNKNOWN;

  if (!Update.begin(beginLen)) {
    binHttp.end();
    setOtaStatus("error", 0.0f, "Update.begin failed", false, true);
    return;
  }

  setOtaStatus("flashing", 0.15f, "Flashing firmware to ESP32...", true, false);

  WiFiClient* stream = binHttp.getStreamPtr();
  uint8_t buf[1024];
  size_t writtenTotal = 0;
  unsigned long lastPacketMs = millis();
  unsigned long lastReportMs = 0;

  while (binHttp.connected() || stream->available() > 0) {
    int avail = stream->available();
    if (avail > 0) {
      int toRead = min((int)sizeof(buf), avail);
      int n = stream->readBytes(buf, toRead);
      if (n > 0) {
        size_t w = Update.write(buf, (size_t)n);
        if (w != (size_t)n) {
          Update.abort();
          binHttp.end();
          setOtaStatus("error", 0.0f, "Flash write error", false, true);
          return;
        }
        writtenTotal += w;
        lastPacketMs = millis();

        unsigned long now = millis();
        if (now - lastReportMs >= 150) {
          lastReportMs = now;
          float prog = 0.5f;
          if (totalLen > 0) {
            float ratio = (float)writtenTotal / (float)totalLen;
            float clampedRatio = constrain(ratio, 0.0f, 1.0f);
            prog = 0.15f + clampedRatio * 0.84f;
          }
          setOtaStatus("flashing", prog, "Flashing firmware (" + tagName + ")...", true, false);
        }
      }
    } else {
      if (totalLen > 0 && (int)writtenTotal >= totalLen) {
        break;
      }
      if (millis() - lastPacketMs > 30000) {
        Update.abort();
        binHttp.end();
        setOtaStatus("error", 0.0f, "Download stream timeout", false, true);
        return;
      }
      delay(2);
    }
  }

  binHttp.end();

  if (totalLen > 0 && (int)writtenTotal != totalLen) {
    Update.abort();
    setOtaStatus("error", 0.0f, "Incomplete firmware download", false, true);
    return;
  }

  if (!Update.end(true) || !Update.isFinished()) {
    setOtaStatus("error", 0.0f, "Update.end failed", false, true);
    return;
  }

  setOtaStatus("done", 1.0f, "ESP32 updated to " + tagName + ". Rebooting...", false, true);
  delay(500);
  ESP.restart();
}

void otaTaskEntry(void* param) {
  runGitHubOtaJob();
  vTaskDelete(nullptr);
}

void handleScan() {
  WiFi.mode(WIFI_AP_STA);
  int n = WiFi.scanNetworks(false, true);
  String json = "{\"ok\":true,\"networks\":[";
  for (int i = 0; i < n; i++) {
    if (i > 0) json += ",";
    String ssid = jsonEscape(WiFi.SSID(i));
    int rssi = WiFi.RSSI(i);
    bool secure = (WiFi.encryptionType(i) != WIFI_AUTH_OPEN);
    json += "{\"ssid\":\"" + ssid + "\",\"rssi\":" + String(rssi) + ",\"secure\":" + (secure ? "true" : "false") + "}";
  }
  json += "]}";
  WiFi.scanDelete();
  server.send(200, "application/json", json);
}

void handleSetupPost() {
  if (!server.hasArg("ssid")) {
    server.send(400, "text/plain", "Missing ssid");
    return;
  }
  String ssid = server.arg("ssid");
  String pass = server.hasArg("password") ? server.arg("password") : "";
  int pin = server.hasArg("pin") ? server.arg("pin").toInt() : DEFAULT_LED_PIN;

  saveConfig(ssid, pass, pin);
  server.send(200, "text/plain", "OK");
  delay(400);
  ESP.restart();
}

void handleVersion() {
  server.send(200, "text/plain", FIRMWARE_VERSION);
}

void handleOtaPull() {
  bool alreadyRunning = false;
  portENTER_CRITICAL(&otaMux);
  alreadyRunning = otaRunning;
  if (!alreadyRunning) {
    otaRunning = true;
    otaDone = false;
    otaProgress = 0.02f;
    otaState = "checking";
    otaMessage = "Starting ESP32 OTA check...";
    otaStartMs = millis();
    otaTriggerPending = true;
  }
  portEXIT_CRITICAL(&otaMux);

  if (alreadyRunning) {
    server.send(202, "application/json", "{\"ok\":true,\"status\":\"already_running\"}");
    return;
  }

  server.send(202, "application/json", "{\"ok\":true,\"status\":\"started\"}");
}

void handleOtaStatus() {
  String st;
  String msg;
  float prog = 0.0f;
  bool running = false;
  bool done = false;
  unsigned long elapsedSec = 0;

  portENTER_CRITICAL(&otaMux);
  st = otaState;
  msg = otaMessage;
  prog = otaProgress;
  running = otaRunning;
  done = otaDone;
  if (otaStartMs > 0) {
    unsigned long endRef = (running || otaStartMs == 0) ? millis() : millis();
    elapsedSec = (endRef - otaStartMs) / 1000UL;
  }
  portEXIT_CRITICAL(&otaMux);

  String json = "{\"state\":\"" + jsonEscape(st) +
                "\",\"progress\":" + String(prog, 3) +
                ",\"running\":" + (running ? "true" : "false") +
                ",\"done\":" + (done ? "true" : "false") +
                ",\"elapsed_sec\":" + String(elapsedSec) +
                ",\"message\":\"" + jsonEscape(msg) + "\"}";
  server.send(200, "application/json", json);
}

void startApProvisioning() {
  apModeActive = true;
  WiFi.disconnect(true);
  WiFi.mode(WIFI_AP);
  WiFi.setSleep(false);
  WiFi.softAP(AP_SSID, AP_PASS);

  server.on("/scan", HTTP_GET, handleScan);
  server.on("/setup", HTTP_POST, handleSetupPost);
  server.on("/version", HTTP_GET, handleVersion);
  server.on("/ota-pull", HTTP_POST, handleOtaPull);
  server.on("/ota-status", HTTP_GET, handleOtaStatus);
  server.begin();
}

void startStationMode() {
  apModeActive = false;
  WiFi.mode(WIFI_STA);
  WiFi.setSleep(false);
  WiFi.begin(savedSsid.c_str(), savedPass.c_str());

  unsigned long t0 = millis();
  while (WiFi.status() != WL_CONNECTED && millis() - t0 < 15000) {
    delay(250);
  }

  if (WiFi.status() != WL_CONNECTED) {
    startApProvisioning();
    return;
  }

  if (MDNS.begin(MDNS_NAME)) {
    MDNS.addService("http", "tcp", 80);
    MDNS.addService("udp", "udp", UDP_PORT);
  }

  server.on("/version", HTTP_GET, handleVersion);
  server.on("/ota-pull", HTTP_POST, handleOtaPull);
  server.on("/ota-status", HTTP_GET, handleOtaStatus);
  server.begin();

  udp.begin(UDP_PORT);
}

void processUdpPackets() {
  int pktSize = udp.parsePacket();
  if (pktSize <= 0) return;

  uint8_t buf[6 + MAX_LEDS * 3];
  int len = udp.read(buf, sizeof(buf));
  if (len <= 0) return;

  if (len == 14 && memcmp(buf, "AMBILIGHT_PING", 14) == 0) {
    return;
  }

  if (len == 15 && memcmp(buf, "AMBILIGHT_RESET", 15) == 0) {
    clearConfigAndReboot();
    return;
  }

  if (len >= 9 && buf[0] == 'A' && buf[1] == 'd' && buf[2] == 'a') {
    uint8_t hi = buf[3];
    uint8_t lo = buf[4];
    uint8_t chk = buf[5];
    if ((hi ^ lo ^ 0x55) != chk) return;

    int count = ((int)hi << 8) + (int)lo + 1;
    if (count <= 0) return;
    if (count > MAX_LEDS) count = MAX_LEDS;

    int expectedBytes = 6 + count * 3;
    if (len < expectedBytes) return;

    numLeds = count;
    for (int i = 0; i < count; i++) {
      int idx = 6 + i * 3;
      leds[i] = CRGB(buf[idx], buf[idx + 1], buf[idx + 2]);
    }
    for (int i = count; i < MAX_LEDS; i++) {
      leds[i] = CRGB::Black;
    }
    FastLED.show();
  }
}

void setup() {
  pinMode(BOOT_BUTTON_PIN, INPUT_PULLUP);

  prefs.begin("ambilight", true);
  bool configured = prefs.getBool("configured", false);
  savedSsid = prefs.getString("ssid", "");
  savedPass = prefs.getString("pass", "");
  savedPin = prefs.getInt("pin", DEFAULT_LED_PIN);
  prefs.end();

  attachLedStrip(savedPin);

  if (!configured || savedSsid.length() == 0) {
    startApProvisioning();
  } else {
    startStationMode();
  }
}

void loop() {
  checkBootButtonReset();
  server.handleClient();

  bool launchTask = false;
  portENTER_CRITICAL(&otaMux);
  if (otaTriggerPending) {
    otaTriggerPending = false;
    launchTask = true;
  }
  portEXIT_CRITICAL(&otaMux);

  if (launchTask) {
    xTaskCreatePinnedToCore(
      otaTaskEntry,
      "GitHubOtaTask",
      8192,
      nullptr,
      1,
      nullptr,
      1
    );
  }

  bool isOtaActive = false;
  portENTER_CRITICAL(&otaMux);
  isOtaActive = otaRunning;
  portEXIT_CRITICAL(&otaMux);

  if (!apModeActive && !isOtaActive) {
    processUdpPackets();
  }
  delay(1);
}