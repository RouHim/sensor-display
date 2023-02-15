#include <Wire.h>
#include <Arduino.h>
#include "U8g2lib.h"

U8G2_SSD1306_128X64_NONAME_F_HW_I2C u8g2(U8G2_R0, /* reset=*/ U8X8_PIN_NONE, A5, /* data=*/ A4);

void drawDataToI2cDisplay(const char *value);

void parseI2cDisplayCommand(String &payload);

void sendToI2cDisplay(String &command);

void setup(void) {
    // Initialize serial communication
    Serial.begin(115200);
}

void loop(void) {
    delay(500);

    // Check if there is data available on the serial port
    // If not, return
    if (Serial.available() <= 0) {
        return;
    }

    String payload = Serial.readStringUntil(';');

    // Print the serial payload that was received
    Serial.print(F("Message received: "));
    Serial.println(payload);

    // Read the first character of the payload
    char messageType = payload.charAt(0);
    Serial.print("messageType: ");
    Serial.println(messageType);

    // Check if the payload starts with a 'i' to identify an i2c command
    if (messageType == 'i') {
        Serial.println("i2c command received");

        // Remove the first character from the payload
        payload.remove(0, 1);

        // Parse the payload as an i2c command
        parseI2cDisplayCommand(payload);
    }
}

/// Parses the payload as an i2c command and sends the data to the displays
/// Example: 0x3C070°C,0x5C051%
/// Syntax: <i2c-address><font-size><data>,<i2c-address><font-size><data>,...
void parseI2cDisplayCommand(String &payload) {
    // Each display data command is separated by a comma
    // Split by comma and iterate over to send the data to the displays
    int commaIndex = payload.indexOf(',');
    Serial.print("commaIndex: ");
    Serial.println(commaIndex);

    // if there is no comma, send the payload to the display
    if (commaIndex == -1) {
        sendToI2cDisplay(payload);
        return;
    }

    while (commaIndex != -1) {
        // Get the display data command
        String displayDataCommand = payload.substring(0, commaIndex);
        Serial.print("displayDataCommand: ");
        Serial.println(displayDataCommand);

        // Send the display data command to the display
        sendToI2cDisplay(displayDataCommand);

        // Remove the display data command from the payload
        payload.remove(0, commaIndex + 1);

        // Find the next comma
        commaIndex = payload.indexOf(',');
        Serial.print("commaIndex: ");
        Serial.println(commaIndex);
    }
}

/// Sends the payload to a display
/// Example payload: 0x3C070°C
void sendToI2cDisplay(String &command) {
    // Print the command that was received

    int length = strlen(command.c_str());
    Serial.print(F("Command received: "));
    Serial.println(command.c_str()[length - 1]);

    /// Determine the i2c address
    // Get the 4 first characters of the payload, without using the substring method
    // Omit the first two characters, because they are just indicating a hex string
    char i2cAddressHexString[3];
    i2cAddressHexString[0] = command.charAt(2);
    i2cAddressHexString[1] = command.charAt(3);
    i2cAddressHexString[2] = '\0';
    // Then convert the hex string to a small integer
    const byte i2cAddress = strtol(i2cAddressHexString, nullptr, 16);
    // Set the i2c address
    u8g2.setI2CAddress(i2cAddress * 2);

    /// Set the font size
    switch (command.charAt(4)) {
        default:
        case 0:
            u8g2.setFont(u8g2_font_profont10_mf);
            break;
        case 1:
            u8g2.setFont(u8g2_font_profont17_mf);
            break;
        case 2:
            u8g2.setFont(u8g2_font_profont29_mf);
            break;
    }

    /// Get the actual data to display
    /// The data starts at the 5th character, without using the substring method
    String data = command.substring(5);
    Serial.print("data: ");
    Serial.println(data);

    // Draw the data to the display
    drawDataToI2cDisplay(data.c_str());
}

void drawDataToI2cDisplay(const char *value) {
    u8g2.clearBuffer();
    u8g2_uint_t x = (u8g2.getDisplayWidth() - u8g2.getUTF8Width(value)) / 2;
    u8g2_uint_t y = (u8g2.getDisplayHeight() - u8g2.getFontAscent() - u8g2.getFontDescent()) / 2 + u8g2.getFontAscent();
    u8g2.drawUTF8(x, y, value);
    u8g2.sendBuffer();
}
