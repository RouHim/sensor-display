#include <Wire.h>
#include <Arduino.h>
#include "U8g2lib.h"

// Initialize the i2c displays that is connected to the ESP8266 D1 and D2 pins
U8G2_SSD1306_128X64_NONAME_F_HW_I2C u8g2(U8G2_R0, /* reset=*/ U8X8_PIN_NONE, /* clock=*/ GPIOR05, /* data=*/ GPIOR04);

void drawDataToI2cDisplay(const char *value);

void parseI2cDisplayCommand(String &payload);

void sendToI2cDisplay(const char *command);

void setup(void) {
    // Initialize serial communication
    Serial.begin(115200);

    // Initialize the i2c displays
    u8g2.begin();
}

void loop(void) {

    // Check if there is data available on the serial port
    // If not, return
    if (Serial.available() <= 0) {
        return;
    }

    String payload = Serial.readStringUntil(';');

    // Check if payload is good, if not, the first two characters are the length of the total payload with the ';'
    byte expectedLength = strtol(payload.substring(0, 2).c_str(), nullptr, 10);
    if (expectedLength != payload.length()) {
        return;
    }

    // If the payload is good, remove the first two characters
    payload.remove(0, 2);

    // Read the first character of the payload
    char messageType = payload.charAt(0);

    // And remove this from the payload
    payload.remove(0, 1);

    // Check if the payload starts with a 'i' to identify an i2c command
    if (messageType == 'i') {

        // Parse the payload as an i2c command
        parseI2cDisplayCommand(payload);
    }

    // Wait a bit
    delay(10);
}

/// Parses the payload as an i2c command and sends the data to the displays
/// Example: 0x3C2137°C,0x5C2137%,...
/// Syntax: <i2c-address><font-size><data>,<i2c-address><font-size><data>,...
void parseI2cDisplayCommand(String &payload) {
    // Each display data command is separated by a comma
    // Split by comma and iterate over to send the data to the displays
    int commaIndex = payload.indexOf(',');

    // if there is no comma, send the payload to the only display
    if (commaIndex == -1) {
        sendToI2cDisplay(payload.c_str());
        return;
    }

    // If there is any (multiple possible), send the data to the corresponding display
    while (commaIndex != -1) {
        // Get the command
        String command = payload.substring(0, commaIndex);
        // Remove the command from the payload
        payload.remove(0, commaIndex + 1);
        // Send the command to the display
        sendToI2cDisplay(command.c_str());

        // Get the next comma index
        commaIndex = payload.indexOf(',');
    }

    // Send the command to the last display
    sendToI2cDisplay(payload.c_str());
}

/// Sends the payload to a display
/// Example payload: 0x3C070°C
void sendToI2cDisplay(const char *command) {
    /// Determine the i2c address
    // First get the hex string, which are the 3rd and 4th characters
    // Ensure the first two characters are 0x if not return with error message
    if (command[0] != '0' || command[1] != 'x') {
        return;
    }

    // Convert only the first 4 characters to a hex string and set the i2c address
    char addressAsHex[4];
    strncpy(addressAsHex, command, 4);

    // Set the i2c address
    u8g2.setI2CAddress(strtol(addressAsHex, nullptr, 16) * 2);

    /// Set the font size that is located in the 5th character
    // WARNING: using three different fonts takes a lot of flash memory
    if (command[4] == '0') {
        u8g2.setFont(u8g2_font_profont10_mf);
    } else if (command[4] == '1') {
        u8g2.setFont(u8g2_font_profont17_mf);
    } else {
        u8g2.setFont(u8g2_font_profont29_mf);
    }

    // Copy the rest of the command to pass it to the draw function
    char dataToShow[strlen(command) - 5];
    strcpy(dataToShow, command + 5);

    // Finally, send the data to the display
    drawDataToI2cDisplay(dataToShow);
}

void drawDataToI2cDisplay(const char *value) {
    u8g2.clearBuffer();
    u8g2_uint_t x = (u8g2.getDisplayWidth() - u8g2.getUTF8Width(value)) / 2;
    u8g2_uint_t y = (u8g2.getDisplayHeight() - u8g2.getFontAscent() - u8g2.getFontDescent()) / 2 + u8g2.getFontAscent();
    u8g2.drawUTF8(x, y, value);
    u8g2.sendBuffer();
}
