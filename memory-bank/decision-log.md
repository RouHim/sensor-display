# Decision Log

## Decision 1
- **Date:** [Date]
- **Context:** [Context]
- **Decision:** [Decision]
- **Alternatives Considered:** [Alternatives]
- **Consequences:** [Consequences]

## Decision 2
- **Date:** [Date]
- **Context:** [Context]
- **Decision:** [Decision]
- **Alternatives Considered:** [Alternatives]
- **Consequences:** [Consequences]

## HTTP Client Registration Update Completed
- **Date:** 2025-07-02 8:09:13 AM
- **Author:** Unknown User
- **Context:** Updated the sensor-display HTTP client to handle the new binary response format from the /api/register endpoint according to CLIENT_REGISTRATION_UPDATE.md
- **Decision:** Successfully migrated from JSON response handling to binary data processing for three types of preparation data: text, static image, and conditional image data
- **Alternatives Considered:** 
  - Keep the old JSON response format
  - Implement a hybrid approach supporting both formats
- **Consequences:** 
  - Fixed TCP to HTTP migration issue
  - Client now receives all necessary static preparation data
  - Proper error handling for both JSON errors and binary processing
  - Added bincode dependency for deserialization

## Client-Server MAC Address Compatibility Issue
- **Date:** 2025-07-04 3:08:39 PM
- **Author:** Unknown User
- **Context:** Analysis of sensor-display client against sensor-bridge server revealed MAC address format mismatch
- **Decision:** Client must normalize MAC address to lowercase before sending to server to match server's normalize_mac_address() function
- **Alternatives Considered:** 
  - Modify server to accept uppercase MACs
  - Leave as-is and document requirement
- **Consequences:** 
  - Ensures compatibility
  - Prevents registration failures
  - Maintains server's consistent MAC format
