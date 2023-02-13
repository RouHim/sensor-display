#include <Wire.h>
#include <Arduino.h>
#include "U8g2lib.h"

U8G2_SSD1306_128X64_NONAME_F_HW_I2C u8g2(U8G2_R0, /* reset=*/ U8X8_PIN_NONE, A5, /* data=*/ A4);

// array of int to store the display addresses
byte DISPLAY_ADDRESSES[2];
byte DISPLAY_FONT_SIZE[2];


void drawDataToI2cDisplay(unsigned int displayIndex, const char *value);

void configureI2cDisplays(const String &message);

void sendToI2cDisplay(const String &message);

void parseDisplayConfig(const String &message, unsigned int index, unsigned int current_char);

void setup(void) {
    // Initialize serial communication
    Serial.begin(115200);
}

void loop(void) {
    // Check if there is data available on the serial port
    if (Serial.available() > 0) {
        String payload = Serial.readStringUntil(';');

        // Print the serial payload that was received
        Serial.print(F("Message received: "));
        Serial.println(payload);

        // Read the first character of the payload
        char messageType = payload.charAt(0);

        // Check if the payload starts with a 'c'
        // If so, configure devices
        if (messageType == 'c') {
            // Remove the first character
            payload.remove(0, 1);

            // If the payload then starts with a 'i', configure the i2c address
            if (payload.charAt(0) == 'i') {
                // Remove the first character
                payload.remove(0, 1);
                // Configure the i2c address
                configureI2cDisplays(payload);
            }
        }
            // Check if the payload starts with a 'd'
            // If so, draw the string to the corresponding display
        else if (messageType == 'd') {
            // Remove the first character
            payload.remove(0, 1);

            // If the payload then starts with a 'i', configure the i2c address
            if (payload.charAt(0) == 'i') {
                // Remove the first character
                payload.remove(0, 1);
                // Draw the string to the corresponding display
                sendToI2cDisplay(payload);
            }
        }
    }

    delay(100);
}

// Draw the given string to the display with the given index
void sendToI2cDisplay(const String &message) {
    // Split the message by comma and draw the string to the corresponding display
    unsigned int index = 0;
    unsigned int start = 0;
    for (unsigned int i = 0; i < message.length(); i++) {
        if (message.charAt(i) == ',') {
            drawDataToI2cDisplay(index, message.substring(start, i).c_str());
            index++;
            start = i + 1;
        }
    }
    drawDataToI2cDisplay(index, message.substring(start, message.length()).c_str());
}

// Configure the display
// Example message: "0x5c,0x5d,0x5e"
void configureI2cDisplays(const String &message) {
    // Split the message into an array, for all the display addresses
    unsigned int displayCount = 1;
    for (unsigned int i = 0; i < message.length(); i++) {
        if (message.charAt(i) == ',') {
            displayCount++;
        }
    }
    unsigned int index = 0;
    unsigned int currentChar = 0;
    for (unsigned int i = 0; i < message.length(); i++) {
        if (message.charAt(i) == ',') {
            parseDisplayConfig(message, index, currentChar);

            index++;
            currentChar = i + 1;
        }
    }
    parseDisplayConfig(message, index, currentChar);

    // Configure the displays
    for (unsigned int i = 0; i < displayCount; i++) {
        u8g2.setI2CAddress(DISPLAY_ADDRESSES[i] * 2);
        u8g2.begin();

        // Parse one of this font sizes 1,2,3
        switch (DISPLAY_FONT_SIZE[i]) {
            case 1:
                u8g2.setFont(u8g2_font_profont10_mf);
            case 2:
                u8g2.setFont(u8g2_font_profont17_mf);
            case 3:
                u8g2.setFont(u8g2_font_profont29_mf);
        }

        // print Hello and the display index to the display
        drawDataToI2cDisplay(i, "...");
    }
}

void parseDisplayConfig(const String &message, unsigned int index, unsigned int current_char) {
    const String &display_config = message.substring(current_char, message.length()).c_str();
    // The first 4 chars of display config represent the display address as hex string prefixed with "0x"
    DISPLAY_ADDRESSES[index] = strtol(
            display_config.substring(0, 4).c_str(),
            nullptr,
            16
    );
    // The last 1 char represent the font size as byte
    DISPLAY_FONT_SIZE[index] = display_config.substring(4, 5).toInt();
}

// Draw the given string to the display with the given index
void drawDataToI2cDisplay(const unsigned int displayIndex, const char *value) {
    u8g2.setI2CAddress(DISPLAY_ADDRESSES[displayIndex] * 2);
    u8g2.clearBuffer();
    u8g2_uint_t x = (u8g2.getDisplayWidth() - u8g2.getUTF8Width(value)) / 2;
    u8g2_uint_t y = (u8g2.getDisplayHeight() - u8g2.getFontAscent() - u8g2.getFontDescent()) / 2 + u8g2.getFontAscent();
    u8g2.drawUTF8(x, y, value);
    u8g2.sendBuffer();
}
