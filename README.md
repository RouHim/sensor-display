# Sensor Display

Sensor Display is part of a two-application system that allows you to display sensor information from one device on
another device's screen. The other part of the system is the [Sensor Bridge](https://github.com/RouHim/sensor-bridge)
application which collects the sensor data.

## Current State

This software is still in development. Expect bugs and unresolved issues.

## Features

* Receives sensor information from the Sensor Bridge application and displays it on the screen.
* Reduces memory and CPU consumption on the device collecting the data, as rendering is offloaded to the device running
  Sensor Display.

## Motivation

This project was born out of the need to display sensor data (such as FPS, CPU load, GPU load, etc.) from a computer on
a separate display. Existing solutions either required payment, didn't support Linux, or rendered the display on the
computer collecting the data, thus consuming resources.

## Architecture

As mentioned before, this system requires two applications:

1. The Sensor Bridge application that runs on the device collecting the sensor data.
2. The Sensor Display application that runs on a separate device (this could be another computer, a Raspberry Pi, or
   similar) with a connected display.

The sensor data is sent from the device running Sensor Bridge to the device running Sensor Display, where it is then
displayed.
