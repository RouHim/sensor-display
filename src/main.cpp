#include <Wire.h>
#include <Arduino.h>
#include "U8g2lib.h"

U8G2_SSD1306_128X64_NONAME_F_HW_I2C u8g2(U8G2_R0, /* reset=*/ U8X8_PIN_NONE, A5, /* data=*/ A4);

u8g2_uint_t displayWidth;
u8g2_uint_t displayHeight;

void drawString(const char *value);

void setup(void) {
    u8g2.begin();
    displayWidth = u8g2.getDisplayWidth();
    displayHeight = u8g2.getDisplayHeight();
    u8g2.setFont(u8g2_font_profont22_mf);
}

void loop(void) {
    drawString("25°C");
    delay(1000);
}

void drawString(const char *value) {
    u8g2.clearBuffer();
    u8g2_uint_t x = (displayWidth - u8g2.getUTF8Width(value)) / 2;
    u8g2_uint_t y = (displayHeight - u8g2.getFontAscent() - u8g2.getFontDescent()) / 2 + u8g2.getFontAscent();
    u8g2.drawUTF8(x, y, value);
    u8g2.sendBuffer();
}
