use prometheus_client::metrics::gauge::Gauge;
use serde::Serialize;
use std::{
    cmp::Eq,
    collections::HashMap,
    hash::Hash,
    marker::{Send, Sync},
    sync::{Arc, RwLock},
};
use strum::IntoEnumIterator;

#[derive(Clone)]
pub struct ReadinessProbe<C: Sync + Send> {
    pub(crate) conditions: Arc<RwLock<HashMap<C, bool>>>,
    pub(crate) up: Box<Gauge<u64>>,
}

impl<C: IntoEnumIterator + Hash + Eq + Send + Sync + Serialize> Default for ReadinessProbe<C> {
    fn default() -> Self {
        let conditions = Arc::new(RwLock::new(C::iter().map(|c| (c, false)).collect()));

        let up = Box::new(Gauge::<u64>::default());
        up.set(0);

        Self { conditions, up }
    }
}

impl<C: IntoEnumIterator + Hash + Eq + Send + Sync + Serialize> ReadinessProbe<C> {
    pub(crate) fn is_ready(&self) -> bool {
        self.conditions
            .read()
            .unwrap()
            .iter()
            .map(|(_, v)| v)
            .all(|v| *v)
    }

    fn set_condition_readiness(&mut self, condition: C, ready: bool) {
        self.conditions.write().unwrap().insert(condition, ready);
        if ready {
            self.up.set(match self.is_ready() {
                true => 1,
                false => 0,
            });
        } else {
            self.up.set(0);
        }
        log::trace!("Condition was set");
    }

    pub fn mark_ready(&mut self, condition: C) {
        self.set_condition_readiness(condition, true);
    }

    pub fn mark_not_ready(&mut self, condition: C) {
        self.set_condition_readiness(condition, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum_macros::EnumIter;

    #[derive(Serialize, PartialEq, Eq, Hash, EnumIter)]
    enum ReadinessConditions {
        One,
        Two,
        Three,
    }

    #[test]
    fn test_basic() {
        let mut rc = ReadinessProbe::<ReadinessConditions>::default();
        assert_eq!(rc.is_ready(), false);
        assert_eq!(rc.up.get(), 0);

        rc.mark_ready(ReadinessConditions::One);
        assert_eq!(rc.is_ready(), false);
        assert_eq!(rc.up.get(), 0);

        rc.mark_ready(ReadinessConditions::Two);
        assert_eq!(rc.is_ready(), false);
        assert_eq!(rc.up.get(), 0);

        rc.mark_ready(ReadinessConditions::Three);
        assert_eq!(rc.is_ready(), true);
        assert_eq!(rc.up.get(), 1);

        rc.mark_not_ready(ReadinessConditions::Two);
        assert_eq!(rc.is_ready(), false);
        assert_eq!(rc.up.get(), 0);
    }
}
