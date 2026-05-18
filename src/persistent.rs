use crate::engine::{Engine, Result};
use crate::wal::{Wal, WalRecord, replay_records};
use std::path::PathBuf;
use std::{collections::BTreeMap, path::Path};

const COMPACTION_THRESHOLD: usize = 100;
pub struct PersistentEngine {
    data: BTreeMap<Vec<u8>, Vec<u8>>,
    wal: Wal,
    path: PathBuf,
    record_count: usize,
    compaction_threshold: usize,
}

impl PersistentEngine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let temp_path = PersistentEngine::compact_temp_path(path);
        PersistentEngine::remove_file_if_exists(&temp_path)?;
        let wal = Wal::open(path)?;
        let records = Wal::load(path)?;
        let record_count = records.len();
        let data = replay_records(records);

        Ok(Self {
            data,
            wal,
            path: path.to_path_buf(),
            record_count,
            compaction_threshold: COMPACTION_THRESHOLD,
        })
    }

    fn maybe_compact(&mut self) -> Result<()> {
        if self.record_count >= self.compaction_threshold {
            self.compact()?;
        }
        Ok(())
    }

    fn compact(&mut self) -> Result<()> {
        let temp_path = PersistentEngine::compact_temp_path(&self.path);
        PersistentEngine::remove_file_if_exists(&temp_path)?;

        {
            let mut compact_wal = Wal::open(&temp_path)?;

            for (key, value) in self.data.iter() {
                let record = WalRecord::Put {
                    key: key.clone(),
                    value: value.clone(),
                };
                compact_wal.append(&record)?;
            }
        }

        std::fs::rename(&temp_path, &self.path)?;
        self.wal = Wal::open(&self.path)?;
        self.record_count = self.data.len();

        Ok(())
    }

    fn compact_temp_path(path: &Path) -> PathBuf {
        path.with_extension("wal.compact")
    }

    fn remove_file_if_exists(path: &Path) -> Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    #[cfg(test)]
    fn temporarily_set_compaction_threshold(&mut self, threshold: usize) {
        self.compaction_threshold = threshold;
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

        self.record_count += 1;
        self.maybe_compact()?;

        Ok(())
    }
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.wal.append(&WalRecord::Delete { key: key.to_vec() })?;

        self.data.remove(key);

        self.record_count += 1;
        self.maybe_compact()?;

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

    #[test]
    fn compact_wal_test() {
        let path = std::env::temp_dir().join("cokekv_compact_wal.log");
        let _ = std::fs::remove_file(&path);

        {
            let mut engine = PersistentEngine::open(&path).unwrap();
            engine.temporarily_set_compaction_threshold(2);
            engine.put(b"k1".to_vec(), b"v1".to_vec()).unwrap();
            engine.put(b"k1".to_vec(), b"v2".to_vec()).unwrap();
        }

        let records = Wal::load(&path).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0],
            WalRecord::Put {
                key: b"k1".to_vec(),
                value: b"v2".to_vec()
            }
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn compact_wal_deleted_record() {
        let path = std::env::temp_dir().join("cokekv_compact_wal_delete_record.log");
        let _ = std::fs::remove_file(&path);

        {
            let mut engine = PersistentEngine::open(&path).unwrap();
            engine.temporarily_set_compaction_threshold(3);
            engine.put(b"k1".to_vec(), b"v1".to_vec()).unwrap();
            engine.put(b"k2".to_vec(), b"v2".to_vec()).unwrap();
            engine.delete(b"k1").unwrap();
        }

        let records = Wal::load(&path).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0],
            WalRecord::Put {
                key: b"k2".to_vec(),
                value: b"v2".to_vec()
            }
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn auto_remove_temp_wal_in_open_and_compact() {
        let path = std::env::temp_dir().join("cokekv_auto_remove.log");
        let temp_path = PersistentEngine::compact_temp_path(&path);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&temp_path);

        {
            let mut wal = Wal::open(&path).unwrap();
            wal.append(&WalRecord::Put {
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
            })
            .unwrap();

            let mut wal_temp = Wal::open(&temp_path).unwrap();
            wal_temp
                .append(&WalRecord::Put {
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
                })
                .unwrap();
        }

        let engine = PersistentEngine::open(&path).unwrap();

        assert!(!temp_path.exists());
        assert_eq!(engine.get(b"k1").unwrap(), Some(b"v1".to_vec()));
        assert_eq!(engine.get(b"k2").unwrap(), None);

        let _ = std::fs::remove_file(&path);
    }
}
