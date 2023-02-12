#include <Wire.h>
#include <Arduino.h>
#include "U8g2lib.h"

U8G2_SSD1306_128X64_NONAME_F_HW_I2C u8g2(U8G2_R0, /* reset=*/ U8X8_PIN_NONE, A5, /* data=*/ A4);

// array of int to store the display addresses
unsigned int displayAddresses[5];

void drawStringToDisplay(unsigned int displayIndex, const char *value);

void configureDisplay(const String &message);

void drawToDisplays(const String &message);

unsigned int toUInt(const char *address_string);

void setup(void) {
    // Initialize serial communication
    Serial.begin(115200);
}

void loop(void) {
    // Check if there is data available on the serial port
    if (Serial.available() > 0) {
        String message = Serial.readStringUntil(';');

        // Print the serial message that was received
        Serial.print("Message received: ");
        Serial.println(message);

        // Read the first character of the message
        char messageType = message.charAt(0);

        // Check if the message starts with a 'c'
        // If so, configure the displays
        if (messageType == 'c') {
            // Print the message type
            Serial.println("Message type: c");

            // Remove the first character
            message.remove(0, 1);
            // Configure displays
            configureDisplay(message);
        }
            // Check if the message starts with a 'd'
            // If so, draw the string to the corresponding display
        else if (messageType == 'd') {
            // Print the message type
            Serial.println("Message type: d");

            // Remove the first character
            message.remove(0, 1);
            // Draw the string to the corresponding display
            // Example message: 5%,50°C
            drawToDisplays(message);
        }
    }

    delay(100);
}

// Draw the given string to the display with the given index
void drawToDisplays(const String &message) {
    // Split the message by comma and draw the string to the corresponding display
    unsigned int index = 0;
    unsigned int start = 0;
    for (unsigned int i = 0; i < message.length(); i++) {
        if (message.charAt(i) == ',') {
            drawStringToDisplay(index, message.substring(start, i).c_str());
            index++;
            start = i + 1;
        }
    }
    drawStringToDisplay(index, message.substring(start, message.length()).c_str());
}

// Configure the display
// Example message: "0x5c,0x5d,0x5e"
void configureDisplay(const String &message) {
    // Split the message into an array
    unsigned int displayCount = 1;
    for (unsigned int i = 0; i < message.length(); i++) {
        if (message.charAt(i) == ',') {
            displayCount++;
        }
    }
    unsigned int index = 0;
    unsigned int start = 0;
    for (unsigned int i = 0; i < message.length(); i++) {
        if (message.charAt(i) == ',') {
            displayAddresses[index] = toUInt(message.substring(start, i).c_str());
            index++;
            start = i + 1;
        }
    }
    // print the substring of the last display address
    displayAddresses[index] = toUInt(message.substring(start, message.length()).c_str());

    // Print the string hex representation of the display addresses
    for (unsigned int i = 0; i < displayCount; i++) {
        Serial.print("Display ");
        Serial.print(i);
        Serial.print(": ");
        Serial.println(displayAddresses[i]);
    }

    // Configure the displays
    for (unsigned int i = 0; i < displayCount; i++) {
        u8g2.setI2CAddress(displayAddresses[i] * 2);
        u8g2.begin();
        u8g2.setFont(u8g2_font_profont22_mf);
        // print Hello and the display index to the display
        drawStringToDisplay(i, "Waiting...");
    }
}

/// Convert the given hex string to an unsigned int
/// Example: "0x5c" -> 92
unsigned int toUInt(const char *address_string) {
    long long_representation = strtol(address_string, nullptr, 16);
    return static_cast<unsigned int>(long_representation);
}

// Draw the given string to the display with the given index
void drawStringToDisplay(const unsigned int displayIndex, const char *value) {
    // Print the string to the serial monitor
    Serial.print("Display ");
    Serial.print(displayIndex);
    Serial.print(": ");
    Serial.println(value);

    u8g2.setI2CAddress(displayAddresses[displayIndex] * 2);
    u8g2.clearBuffer();
    u8g2_uint_t x = (u8g2.getDisplayWidth() - u8g2.getUTF8Width(value)) / 2;
    u8g2_uint_t y = (u8g2.getDisplayHeight() - u8g2.getFontAscent() - u8g2.getFontDescent()) / 2 + u8g2.getFontAscent();
    u8g2.drawUTF8(x, y, value);
    u8g2.sendBuffer();
}
