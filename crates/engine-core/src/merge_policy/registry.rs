//! The closed set of strategies a policy may name. A policy that names anything
//! else is rejected when it is loaded and fails closed when it is run.

use super::strategy::Strategy;

pub struct Registry {
    strategies: Vec<Box<dyn Strategy>>,
}

/// The id was already registered.
#[derive(Debug, PartialEq, Eq)]
pub struct Duplicate(pub &'static str);

impl Registry {
    pub fn empty() -> Self {
        Self {
            strategies: Vec::new(),
        }
    }

    /// Every strategy that is built: the three that delegate to the existing
    /// merge logic and the element-wise collection strategy.
    pub fn builtin() -> Self {
        let mut registry = Self::empty();
        for strategy in super::default::strategies() {
            registry
                .register(strategy)
                .expect("built-in strategy ids are unique");
        }
        registry
            .register(Box::new(super::collections::ElementWise))
            .expect("built-in strategy ids are unique");
        registry
    }

    pub fn register(&mut self, strategy: Box<dyn Strategy>) -> Result<(), Duplicate> {
        let id = strategy.id();
        if self.get(id).is_some() {
            return Err(Duplicate(id));
        }
        self.strategies.push(strategy);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&dyn Strategy> {
        self.strategies
            .iter()
            .find(|s| s.id() == id)
            .map(|s| s.as_ref())
    }

    /// The registered ids, sorted.
    pub fn ids(&self) -> Vec<&'static str> {
        let mut ids: Vec<_> = self.strategies.iter().map(|s| s.id()).collect();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_ids_are_unique_versioned_and_sorted() {
        let registry = Registry::builtin();
        let ids = registry.ids();
        assert_eq!(
            ids,
            [
                "dynodoc.collection.element-wise.v1",
                "dynodoc.default.property.v1",
                "dynodoc.default.richtext.v1",
                "dynodoc.default.whole-value.v1",
            ]
        );
        assert!(ids
            .iter()
            .all(|id| id.starts_with("dynodoc.") && id.ends_with(".v1")));
        for id in ids {
            assert_eq!(registry.get(id).unwrap().id(), id);
        }
        assert!(registry.get("dynodoc.nope.v1").is_none());
    }

    #[test]
    fn a_second_registration_of_an_id_is_refused() {
        let mut registry = Registry::builtin();
        let again = Box::new(crate::merge_policy::collections::ElementWise);
        assert_eq!(
            registry.register(again),
            Err(Duplicate("dynodoc.collection.element-wise.v1"))
        );
        assert_eq!(registry.ids().len(), 4);
    }
}
