use std::collections::BTreeMap;

use crate::engine::{Engine, Result};

pub struct MemoryEngine {
    data: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl MemoryEngine {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }
}

impl Default for MemoryEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for MemoryEngine {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.data.get(key) {
            Some(value) => Ok(Some(value.clone())),
            None => Ok(None),
        }
    }

    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.data.insert(key, value);
        Ok(())
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.data.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_engine_basic_behaviors<E: Engine>(engine: &mut E) {
        // put/get
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

    #[test]
    fn memory_engine_basic_behaviors() {
        let mut engine = MemoryEngine::new();
        assert_engine_basic_behaviors(&mut engine);
    }
}
