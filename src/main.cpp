#include <Wire.h>
#include <Adafruit_SSD1306.h>
#include <Adafruit_GFX.h>

#define SCREEN_WIDTH 128 // OLED display width, in pixels
#define SCREEN_HEIGHT 64 // OLED display height, in pixels

// Declaration for an SSD1306 display connected to I2C (SDA, SCL pins)
Adafruit_SSD1306 display(SCREEN_WIDTH, SCREEN_HEIGHT, &Wire, -1);

void showMessage(const String &message);

void setup() {
    // Initialize serial with baud rate 115200
    Serial.begin(115200);

    if (!display.begin(SSD1306_SWITCHCAPVCC, 0x3C)) {
        Serial.println(F("SSD1306 allocation failed"));
        for (;;);
    }

    // Wait for display to initialize
    delay(1000);

    // Clear the display
    display.clearDisplay();

    // Set text font style
    display.setTextSize(1);
    display.setTextColor(WHITE);

    // Set text cursor position
    display.setCursor(0, 0);
    // Print current time
    display.println("Wait for a new serial message to arrive...");

    // Swap the buffer
    display.display();

    // Set text size for following text
    display.setTextSize(3);
}

/// Main loop
void loop() {
    // Check if there is a new serial message
    if (Serial.available() > 0) {
        String message = Serial.readStringUntil(';');
        showMessage(message);
    }
}

/// Show a message on the OLED display
void showMessage(const String &message) {
    // Clear the display
    display.clearDisplay();

    // Calculate the center of the screen
    int16_t x1, y1;
    uint16_t w, h;
    display.getTextBounds(message, 0, 0, &x1, &y1, &w, &h);
    int16_t x = static_cast<int16_t>(display.width() - w) / 2;
    int16_t y = static_cast<int16_t>(display.height() - h) / 2;

    // Set text cursor position to center
    display.setCursor(x, y);

    // Print message
    display.println(message);

    // Swap the buffer
    display.display();
}
