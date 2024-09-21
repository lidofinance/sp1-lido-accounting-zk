pub mod serde_hex_as_string {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    pub struct FixedHexStringProtocol<const N: usize> {}

    impl<const N: usize> FixedHexStringProtocol<N> {
        pub fn serialize<'a, S>(value: &'a [u8], serializer: S) -> Result<S::Ok, S::Error>
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
        pub fn serialize<'a, S>(value: &'a Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
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
}
