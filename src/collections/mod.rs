// src/collections/mod.rs

use core::fmt;

// Provide your custom HashMap
#[derive(Clone)]  // if you want cloning
pub struct HashMap<K, V, const CAP: usize> {
    // Instead of [Option<(K, V)>; CAP], let's store a Vec<Option<(K, V)>>
    // to avoid the 'K: Copy' or 'V: Copy' requirement.
    // Then we can implement .init() easily as well.
    entries: Vec<Option<(K, V)>>,
}

// Let's also implement Debug:
impl<K: fmt::Debug, V: fmt::Debug, const CAP: usize> fmt::Debug for HashMap<K, V, CAP> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HashMap")
         .field("entries", &self.entries)
         .finish()
    }
}

impl<K: PartialEq, V, const CAP: usize> HashMap<K, V, CAP> {
    pub fn init() -> Self {
        // Start with a vec of capacity CAP, each = None
        let mut v = Vec::new();
        v.resize_with(CAP, || None);
        Self { entries: v }
    }

    pub fn new() -> Self {
        Self::init()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().filter_map(|slot| slot.as_ref().map(|(k, v)| (k, v)))
    }

    pub fn insert(&mut self, key: K, value: V) {
        // Minimal linear search for empty or same key
        for slot in self.entries.iter_mut() {
            if let Some((ref stored_k, _)) = slot {
                if *stored_k == key {
                    // replace
                    *slot = Some((key, value));
                    return;
                }
            } else {
                *slot = Some((key, value));
                return;
            }
        }
        // if no slot found, do nothing or expand
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        for slot in &self.entries {
            if let Some((ref stored_k, ref stored_v)) = slot {
                if *stored_k == *key {
                    return Some(stored_v);
                }
            }
        }
        None
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        for slot in &mut self.entries {
            if let Some((ref stored_k, ref mut stored_v)) = slot {
                if *stored_k == *key {
                    return Some(stored_v);
                }
            }
        }
        None
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        for slot in &mut self.entries {
            if let Some((ref stored_k, _)) = slot {
                if *stored_k == *key {
                    let old = slot.take().unwrap().1;
                    return Some(old);
                }
            }
        }
        None
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    pub fn clear(&mut self) {
        for slot in &mut self.entries {
            *slot = None;
        }
    }
}

