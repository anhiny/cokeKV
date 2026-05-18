use crate::engine::{Error, Result};
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

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

    let mut encoded = Vec::with_capacity(HEADER_LEN + key.len() + value.len());
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
        RECORD_TYPE_PUT => {
            let key = bytes[key_start..key_end].to_vec();
            let value = bytes[val_start..val_end].to_vec();

            Ok(WalRecord::Put { key, value })
        }
        RECORD_TYPE_DELETE => {
            if val_len != 0 {
                return Err(Error::InvalidWalRecord);
            }
            let key = bytes[key_start..key_end].to_vec();

            Ok(WalRecord::Delete { key })
        }
        _ => Err(Error::InvalidWalRecord),
    }
}

pub fn decode_records(bytes: &[u8]) -> Result<Vec<WalRecord>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut offset = 0;
    let mut records = Vec::new();
    while offset < bytes.len() {
        if bytes.len() - offset < HEADER_LEN {
            return Err(Error::InvalidWalRecord);
        }
        let key_len =
            u32::from_le_bytes(bytes[offset + 1..offset + 5].try_into().unwrap()) as usize;
        let value_len =
            u32::from_le_bytes(bytes[offset + 5..offset + 9].try_into().unwrap()) as usize;
        let record_len = HEADER_LEN + key_len + value_len;

        if offset + record_len > bytes.len() {
            return Err(Error::InvalidWalRecord);
        }
        let record = decode_record(&bytes[offset..offset + record_len])?;
        records.push(record);
        offset += record_len;
    }
    Ok(records)
}

pub struct Wal {
    file: std::fs::File,
}

impl Wal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;

        Ok(Self { file })
    }

    pub fn append(&mut self, record: &WalRecord) -> Result<()> {
        let encoded = encode_record(record);
        self.file.write_all(&encoded)?;
        self.file.sync_all()?;

        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Vec<WalRecord>> {
        let bytes = std::fs::read(path)?;
        decode_records(&bytes)
    }
}

pub fn replay_records(records: Vec<WalRecord>) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut data = BTreeMap::new();

    for record in records {
        match record {
            WalRecord::Put { key, value } => {
                data.insert(key, value);
            }
            WalRecord::Delete { key } => {
                data.remove(&key);
            }
        }
    }

    data
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

    #[test]
    fn encode_then_decode_delete_record() {
        let record = WalRecord::Delete {
            key: b"k1".to_vec(),
        };

        let encoded = encode_record(&record);
        let decoded = decode_record(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn decode_reject_too_short_record() {
        let bytes = vec![RECORD_TYPE_PUT];

        assert!(decode_record(&bytes).is_err());
    }

    #[test]
    fn decode_reject_unknown_type_record() {
        let mut bytes = Vec::new();
        bytes.push(99);
        bytes.extend((0u32).to_le_bytes());
        bytes.extend((0u32).to_le_bytes());

        assert!(decode_record(&bytes).is_err());
    }

    #[test]
    fn decode_reject_length_mismatch() {
        let mut bytes = Vec::new();
        bytes.push(RECORD_TYPE_PUT);
        bytes.extend((2u32).to_le_bytes());
        bytes.extend((0u32).to_le_bytes());

        assert!(decode_record(&bytes).is_err());
    }

    #[test]
    fn decode_reject_with_value_delete_record() {
        let mut bytes = Vec::new();
        bytes.push(RECORD_TYPE_DELETE);
        bytes.extend((2u32).to_le_bytes());
        bytes.extend((1u32).to_le_bytes());
        bytes.extend(b"k1");
        bytes.extend(b"v");

        assert!(decode_record(&bytes).is_err());
    }

    #[test]
    fn wal_append_writes_encoded_record_to_file() {
        let path = std::env::temp_dir().join("cokekv_wal_append_test.log");

        let _ = std::fs::remove_file(&path);

        let mut wal = Wal::open(&path).unwrap();

        let record = WalRecord::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        };

        wal.append(&record).unwrap();

        drop(wal);

        let bytes = std::fs::read(&path).unwrap();

        assert_eq!(encode_record(&record), bytes);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn decode_multiple_records() {
        let record1 = WalRecord::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        };

        let record2 = WalRecord::Delete {
            key: b"k1".to_vec(),
        };

        let mut bytes = Vec::new();
        bytes.extend(encode_record(&record1));
        bytes.extend(encode_record(&record2));

        let records = decode_records(&bytes).unwrap();
        assert_eq!(records, vec![record1, record2]);
    }

    #[test]
    fn decode_empty_records() {
        assert_eq!(decode_records(&[]).unwrap(), Vec::new());
    }

    #[test]
    fn decode_reject_trailing_partial_header() {
        let record = WalRecord::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        };

        let mut bytes = encode_record(&record);
        bytes.extend(vec![1, 2, 3]);

        assert!(decode_records(&bytes).is_err());
    }

    #[test]
    fn decode_reject_non_empty_partial_header() {
        let bytes = vec![RECORD_TYPE_PUT];

        assert!(decode_records(&bytes).is_err());
    }

    #[test]
    fn wal_load_reads_appended_records() {
        let path = std::env::temp_dir().join("cokekv_wal_load");
        let _ = std::fs::remove_file(&path);

        let record1 = WalRecord::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        };

        let record2 = WalRecord::Delete {
            key: b"k2".to_vec(),
        };

        let mut wal = Wal::open(&path).unwrap();
        wal.append(&record1).unwrap();
        wal.append(&record2).unwrap();

        drop(wal);

        let loaded = Wal::load(&path).unwrap();
        assert_eq!(loaded, vec![record1, record2]);

        std::fs::remove_file(&path).unwrap();
    }
    #[test]
    fn replay_put_record() {
        let records = vec![WalRecord::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        }];

        let data = replay_records(records);

        assert_eq!(data.get(b"k1".as_slice()), Some(&b"v1".to_vec()));
    }

    #[test]
    fn replay_inorder_record() {
        let records = vec![
            WalRecord::Put {
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
            },
            WalRecord::Put {
                key: b"k1".to_vec(),
                value: b"v2".to_vec(),
            },
            WalRecord::Delete {
                key: b"k1".to_vec(),
            },
        ];

        let data = replay_records(records);

        assert!(data.get(b"k1".as_slice()).is_none());
    }
}
