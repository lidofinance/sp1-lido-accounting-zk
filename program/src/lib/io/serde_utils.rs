pub mod serde_hex_as_string {
    use serde::de::Error;
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serializer};

    pub struct FixedHexStringProtocol<const N: usize> {}

    impl<const N: usize> FixedHexStringProtocol<N> {
        pub fn serialize<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let res = format!("0x{}", hex::encode(value));
            serializer.serialize_str(&res)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; N], D::Error>
        where
            D: Deserializer<'de>,
        {
            let mut s: &str = Deserialize::deserialize(deserializer)?;
            s = s.strip_prefix("0x").unwrap_or(s);
            let mut slice: [u8; N] = [0; N];
            hex::decode_to_slice(s, &mut slice).map_err(Error::custom)?;
            Ok(slice)
        }
    }

    pub struct HexStringProtocol {}
    impl HexStringProtocol {
        pub fn serialize<S>(value: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let res = format!("0x{}", hex::encode(value));
            serializer.serialize_str(&res)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let mut s: &str = Deserialize::deserialize(deserializer)?;
            s = s.strip_prefix("0x").unwrap_or(s);
            let decoded = hex::decode(s).map_err(Error::custom)?;
            Ok(decoded)
        }
    }

    pub struct VecOfHexStringProtocol {}
    impl VecOfHexStringProtocol {
        pub fn serialize<S>(value: &Vec<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(value.len()))?;
            for element in value {
                let res = format!("0x{}", hex::encode(element));
                seq.serialize_element(&res)?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let values: Vec<&str> = Deserialize::deserialize(deserializer)?;
            let mut result: Vec<Vec<u8>> = Vec::with_capacity(values.len());
            for hex_str in values {
                let raw_hex = hex_str.strip_prefix("0x").unwrap_or(hex_str);
                let decoded = hex::decode(raw_hex).map_err(Error::custom)?;
                result.push(decoded);
            }

            Ok(result)
        }
    }
}
