#include <Wire.h>
#include <Adafruit_SSD1306.h>

#define SCREEN_WIDTH 128 // OLED display width, in pixels
#define SCREEN_HEIGHT 64 // OLED display height, in pixels

// Declaration for an SSD1306 display connected to I2C (SDA, SCL pins)
Adafruit_SSD1306 display(SCREEN_WIDTH, SCREEN_HEIGHT, &Wire, -1);

void setup() {
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
    // Print text
    display.println("Wait for a new serial message to arrive...");

    // Swap the buffer
    display.display();
}

void loop() {
    // Wait for a new serial message to arrive
    if (Serial.available() > 0) {
        // Read the incoming message
        String message = Serial.readString();
        // Clear the display
        display.clearDisplay();

        // (Re)set the cursor position
        display.setCursor(0, 0);
        // Display the message on the OLED display
        display.println(message);
        // Update the OLED display
        display.display();
        // Wait for 500ms
        delay(500);
    }
}
