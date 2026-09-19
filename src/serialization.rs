/// Deserializes the static data payload delivered by the bridge.
/// The bridge writes with bincode's legacy config (little endian, fixed-width
/// integers); `config::standard()` uses varint encoding and cannot read it.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let (value, _bytes_read) =
        bincode_next::serde::decode_from_slice::<T, _>(bytes, bincode_next::config::legacy())
            .map_err(|err| format!("Failed to deserialize static data: {}", err))?;

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sensor_core::StaticClientData;
    use std::collections::HashMap;

    #[test]
    fn decodes_the_bincode_1_wire_format() {
        // Exactly the bytes the bridge's `config::legacy()` writer produces for
        // `vec![1u8, 2, 3]` (u64 length prefix, little endian). `config::standard()`
        // would use varint and fail here.
        let bytes = [3u8, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3];

        let value: Vec<u8> = decode(&bytes).expect("legacy bytes must decode");

        assert_eq!(value, vec![1, 2, 3]);
    }

    #[test]
    fn static_client_data_round_trips() {
        let mut text_data = HashMap::new();
        text_data.insert("Arial".to_string(), ("a1b2".to_string(), vec![1u8, 2, 3]));
        let static_data = StaticClientData {
            text_data,
            static_image_data: HashMap::new(),
            conditional_image_data: HashMap::new(),
        };

        let bytes =
            bincode_next::serde::encode_to_vec(&static_data, bincode_next::config::legacy())
                .expect("encoding must succeed");
        let decoded: StaticClientData = decode(&bytes).expect("decoding must succeed");

        assert_eq!(decoded.text_data, static_data.text_data);
    }

    #[test]
    fn garbage_fails_to_decode() {
        assert!(decode::<StaticClientData>(&[0xff, 0x00, 0x13]).is_err());
    }
}
