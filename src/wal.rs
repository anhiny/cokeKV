use crate::engine::{Error, Result};
use std::convert::TryInto;

#[derive(Debug, PartialEq, Eq)]
pub enum WalRecord {
    Put { key: Vec<u8>, value: Vec<u8> },

    Delete { key: Vec<u8> },
}

const RECORD_TYPE_PUT: u8 = 1;
const RECORD_TYPE_DELETE: u8 = 2;
const HEADER_LEN: usize = 1 + 4 + 4;

/* encode format
    record_type: 1 byte
    key_len:     4 bytes
    value_len:   4 bytes
    key bytes
    value bytes
*/

pub fn encode_record(record: &WalRecord) -> Vec<u8> {
    let (record_type, key, value): (u8, &[u8], &[u8]) = match record {
        WalRecord::Put { key, value } => (RECORD_TYPE_PUT, key, value),
        WalRecord::Delete { key } => (RECORD_TYPE_DELETE, key, &[]),
    };

    let key_len = key.len() as u32;
    let key_len_bytes = key_len.to_le_bytes();
    let val_len: u32 = value.len() as u32;
    let val_len_bytes = val_len.to_le_bytes();

    let mut encoded = Vec::with_capacity(1 + 4 + 4 + key.len() + value.len());
    encoded.push(record_type);
    encoded.extend(key_len_bytes);
    encoded.extend(val_len_bytes);
    encoded.extend(key);
    encoded.extend(value);

    encoded
}

pub fn decode_record(bytes: &[u8]) -> Result<WalRecord> {
    if bytes.len() < HEADER_LEN {
        return Err(Error::InvalidWalRecord);
    }

    let key_len = u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
    let val_len = u32::from_le_bytes(bytes[5..9].try_into().unwrap()) as usize;

    let total_len = HEADER_LEN + key_len + val_len;
    if bytes.len() != total_len {
        return Err(Error::InvalidWalRecord);
    }

    let key_start = HEADER_LEN;
    let key_end = key_start + key_len;
    let val_start = key_end;
    let val_end = val_start + val_len;

    match bytes[0] {
        1 => {
            let key = bytes[key_start..key_end].to_vec();
            let value = bytes[val_start..val_end].to_vec();

            Ok(WalRecord::Put { key, value })
        }
        2 => {
            if val_len != 0 {
                return Err(Error::InvalidWalRecord);
            }
            let key = bytes[key_start..key_end].to_vec();

            Ok(WalRecord::Delete { key })
        }
        _ => Err(Error::InvalidWalRecord),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_put_record() {
        let record = WalRecord::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        };

        let encoded = encode_record(&record);

        assert_eq!(encoded[0], RECORD_TYPE_PUT);
        assert_eq!(&encoded[1..5], &(2u32.to_le_bytes()));
        assert_eq!(&encoded[5..9], &(2u32.to_le_bytes()));
        assert_eq!(&encoded[9..11], b"k1");
        assert_eq!(&encoded[11..13], b"v1");
    }

    #[test]
    fn encode_delete_record() {
        let record = WalRecord::Delete {
            key: b"k1".to_vec(),
        };

        let encoded = encode_record(&record);

        assert_eq!(encoded[0], RECORD_TYPE_DELETE);
        assert_eq!(&encoded[1..5], &(2u32.to_le_bytes()));
        assert_eq!(&encoded[5..9], &(0u32.to_le_bytes()));
        assert_eq!(&encoded[9..11], b"k1");
        assert_eq!(encoded.len(), 11);
        assert_eq!(&encoded[11..], b"");
    }

    #[test]
    fn encode_then_decode_put_record() {
        let record = WalRecord::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        };

        let encoded = encode_record(&record);
        let decoded = decode_record(&encoded).unwrap();

        assert_eq!(decoded, record);
    }
}
