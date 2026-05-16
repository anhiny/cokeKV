use crate::engine::{Engine, Result};
use crate::wal::{Wal, WalRecord, replay_records};
use std::{collections::BTreeMap, path::Path};
pub struct PersistentEngine {
    data: BTreeMap<Vec<u8>, Vec<u8>>,
    wal: Wal,
}

impl PersistentEngine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let wal = Wal::open(path)?;
        let records = Wal::load(path)?;
        let data = replay_records(records);

        Ok(Self { data, wal })
    }
}

impl Engine for PersistentEngine {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.data.get(key) {
            Some(values) => Ok(Some(values.clone())),
            None => Ok(None),
        }
    }
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.wal.append(&WalRecord::Put {
            key: key.clone(),
            value: value.clone(),
        })?;

        self.data.insert(key, value);
        Ok(())
    }
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.wal.append(&WalRecord::Delete { key: key.to_vec() })?;

        self.data.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    #[test]
    fn persistent_engine_basic_behaviors() {
        let path = std::env::temp_dir().join("cokekv_persistent_basic.log");
        let _ = std::fs::remove_file(&path);
        {
            let mut engine = PersistentEngine::open(&path).unwrap();

            engine.put(b"k1".to_vec(), b"v1".to_vec()).unwrap();

            let value = engine.get(b"k1").unwrap();

            assert_eq!(value, Some(b"v1".to_vec()));

            // overwrite
            engine.put(b"k1".to_vec(), b"v2".to_vec()).unwrap();

            assert_eq!(engine.get(b"k1").unwrap(), Some(b"v2".to_vec()));

            // delete
            engine.delete(b"k1").unwrap();

            assert_eq!(engine.get(b"k1").unwrap(), None);

            // miss key
            assert_eq!(engine.get(b"k1").unwrap(), None);

            assert_eq!(engine.get(b"missing").unwrap(), None);

            // empty key/value
            engine.put(b"".to_vec(), b"v1".to_vec()).unwrap();

            assert_eq!(engine.get(b"").unwrap(), Some(b"v1".to_vec()));

            engine.put(b"k1".to_vec(), b"".to_vec()).unwrap();

            assert_eq!(engine.get(b"k1").unwrap(), Some(b"".to_vec()));

            // delete miss key
            assert!(engine.delete(b"missing").is_ok());
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persistent_engine_recovers_from_wal() {
        let path = std::env::temp_dir().join("cokekv_persistent_recover.log");
        let _ = std::fs::remove_file(&path);

        {
            let mut engine = PersistentEngine::open(&path).unwrap();
            engine.put(b"k1".to_vec(), b"v1".to_vec()).unwrap();
            engine.put(b"k2".to_vec(), b"v2".to_vec()).unwrap();
            engine.delete(b"k1").unwrap();
        }

        {
            let engine = PersistentEngine::open(&path).unwrap();
            assert!(engine.get(b"k1").unwrap().is_none());
            assert_eq!(engine.get(b"k2").unwrap(), Some(b"v2".to_vec()));
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persistent_engine_recovers_latest_value_after_overwrite() {
        let path = std::env::temp_dir().join("cokekv_persistent_overwrite.log");
        let _ = std::fs::remove_file(&path);

        {
            let mut engine = PersistentEngine::open(&path).unwrap();
            engine.put(b"k1".to_vec(), b"v1".to_vec()).unwrap();
            engine.put(b"k1".to_vec(), b"v2".to_vec()).unwrap();
        }

        {
            let engine = PersistentEngine::open(&path).unwrap();
            assert_eq!(engine.get(b"k1").unwrap(), Some(b"v2".to_vec()));
        }

        let _ = std::fs::remove_file(&path);
    }
}
