# Aura Pro

**Aura Pro** is a real-time screen-reactive ambient lighting system for Windows powered by an **ESP32** microcontroller running **FastLED** and a **Rust (`egui`)** desktop companion application.

Developed and maintained by **Ganith Pasindu**.

---

## Features

- **Real-Time Screen Capture (20 FPS):** Samples screen zones and streams color frames over UDP using an Adalight-style packet header (`Ada` + checksum) on port `4210`.
- **Guided Initial Setup:** Built-in AP provisioning mode (`Aura Pro` / `Aura1234`), Wi-Fi network scanning, and selectable WS2812B data GPIO pins (`2, 4, 5, 13–19, 21–23, 25`).
- **LED Layout & Per-LED Calibration:** Configure Top, Bottom, Left, and Right LED counts, start corner, rotation direction (CW / CCW), display target, and per-LED position offsets.
- **Tuning & Auto-Start:** Adjust brightness, gamma, white balance (`R / G / B`), and enable automatic streaming on startup.
- **System Tray & Updates:** Minimize to system tray, check/install desktop app updates (`Aura_pro_Setup.exe`), and trigger ESP32 OTA firmware updates (`aura_pro_esp32.bin`).

---

## Hardware & Wiring Guide

| Signal / Pin | Connection | Notes |
| :--- | :--- | :--- |
| **WS2812B DIN** | ESP32 GPIO Pin (default `GPIO 18`) | Configurable during setup |
| **WS2812B 5V** | External 5V Power Supply (+) | Avoid powering long LED strips from ESP32 USB 5V |
| **GND** | Common Ground (ESP32 GND + 5V PSU − + LED GND) | Required for reliable data signal |
| **ESP32 BOOT** | `GPIO 0` | Hold ~3 seconds (magenta LED flash) to factory reset |

---

## Quick Start

1. **Flash ESP32 Firmware:** Flash the ESP32 firmware (`aura_pro_esp32.bin`) via Arduino IDE / ESP Web Tools / OTA.
2. **AP Mode Setup:** Connect your PC to Wi-Fi **`Aura Pro`** (password: **`Aura1234`**).
3. **Configure Wi-Fi & GPIO:** Open **Aura Pro**, scan/select your home Wi-Fi, enter the password, choose your LED GPIO pin, and save.
4. **Home Wi-Fi & Calibration:** Reconnect your PC to your home Wi-Fi, set your LED layout, run **Position Calibration** and **Tuning & Auto-Start**, then click **Start**.

---

## Building from Source

### Desktop App (Rust)
```bash
cargo build --release
