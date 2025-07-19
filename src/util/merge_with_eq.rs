use anyhow::Result;
use std::collections::HashMap;
use std::hash::Hash;

/// Defines a trait for merging two hashmaps where values for common keys must be equal.
///
/// This trait provides a contract for types that can be merged with another `HashMap`.
/// The core rule of the merge is that if a key exists in both maps, their corresponding
/// values must be equal for the merge to succeed.
pub trait MergeWithEq<K, V>
where
    K: Eq + Hash + Clone,
    V: Eq + Clone,
{
    /// Merges two hashmaps, returning a new hashmap.
    ///
    /// This method combines the key-value pairs from `self` and `other`.
    ///
    /// # Behavior
    /// - Keys that exist only in `self` are included.
    /// - Keys that exist only in `other` are included.
    /// - Keys that exist in both `self` and `other` are included, but only if their
    ///   values are equal (`v_self == v_other`).
    ///
    /// # Returns
    /// - `Ok(HashMap<K, V>)`: A new `HashMap` containing the merged key-value pairs
    ///   if the merge is successful.
    /// - `Err(String)`: An error message describing the conflict if any common key
    ///   has mismatched values.
    #[allow(dead_code)]
    fn merge_with_eq(&self, other: &HashMap<K, V>) -> Result<HashMap<K, V>>;
}

/// Implements the `MergeWithEq` trait for `std::collections::HashMap`.
impl<K, V> MergeWithEq<K, V> for HashMap<K, V>
where
    // Add `Debug` trait bound to format keys and values in the error message.
    K: Eq + Hash + Clone + std::fmt::Debug,
    V: Eq + Clone + std::fmt::Debug,
{
    /// Merges `self` with another `HashMap`.
    ///
    /// The implementation uses a two-pass approach to efficiently merge the maps
    /// and detect conflicts.
    fn merge_with_eq(&self, other: &HashMap<K, V>) -> Result<HashMap<K, V>> {
        // Pre-allocate capacity for the new map to avoid reallocations.
        // This is an optimization for performance.
        let mut merged = HashMap::with_capacity(self.len() + other.len());

        // First pass: Iterate over `self`'s key-value pairs.
        // This pass handles all keys from `self` and checks for conflicts with `other`.
        for (k, v_self) in self {
            // Check if the key from `self` also exists in `other`.
            if let Some(v_other) = other.get(k) {
                // If the key exists in both maps, their values must be equal.
                if v_self != v_other {
                    // If values are not equal, a conflict is found. Return an error.
                    return Err(anyhow::anyhow!(
                        "value mismatch for key {:?}: left={:?}, right={:?}",
                        k,
                        v_self,
                        v_other
                    ));
                }
            }
            // Insert the key-value pair from `self` into the merged map.
            // This covers both keys unique to `self` and common keys that have passed the equality check.
            merged.insert(k.clone(), v_self.clone());
        }

        // Second pass: Iterate over `other`'s key-value pairs.
        // This pass is only responsible for adding keys that are unique to `other`.
        for (k, v_other) in other {
            // Check if the key from `other` is NOT already in the merged map.
            // Keys common to both maps were already added in the first pass.
            if !merged.contains_key(k) {
                // If the key is unique to `other`, insert it into the merged map.
                merged.insert(k.clone(), v_other.clone());
            }
        }

        // If both passes complete without returning an error, the merge was successful.
        Ok(merged)
    }
}
