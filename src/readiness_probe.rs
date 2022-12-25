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
use strum_macros::EnumIter;

#[derive(Clone, Serialize, PartialEq, Eq, Hash, EnumIter)]
pub enum AlwaysReady {}

#[derive(Clone)]
pub struct ReadinessProbe<C: Sync + Send> {
    pub(crate) conditions: Arc<RwLock<HashMap<C, bool>>>,
    pub(crate) up: Gauge<i64>,
}

impl<C: IntoEnumIterator + Hash + Eq + Send + Sync + Serialize> Default for ReadinessProbe<C> {
    fn default() -> Self {
        let conditions = Arc::new(RwLock::new(C::iter().map(|c| (c, false)).collect()));
        let up = Gauge::<i64>::default();

        let mut probe = Self { conditions, up };
        probe.update_up_metric();

        probe
    }
}

impl<C: IntoEnumIterator + Hash + Eq + Send + Sync + Serialize> ReadinessProbe<C> {
    pub(crate) fn is_ready(&self) -> bool {
        let conditions = self.conditions.read().unwrap();
        if conditions.is_empty() {
            true
        } else {
            conditions.iter().map(|(_, v)| v).all(|v| *v)
        }
    }

    fn update_up_metric(&mut self) {
        self.up.set(match self.is_ready() {
            true => 1,
            false => 0,
        });
    }

    fn set_condition_readiness(&mut self, condition: C, ready: bool) {
        self.conditions.write().unwrap().insert(condition, ready);
        if ready {
            self.update_up_metric();
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

    #[derive(Serialize, PartialEq, Eq, Hash, EnumIter)]
    enum ReadinessConditions {
        One,
        Two,
        Three,
    }

    #[test]
    fn test_basic() {
        let mut rc = ReadinessProbe::<ReadinessConditions>::default();
        assert!(!rc.is_ready());
        assert_eq!(rc.up.get(), 0);

        rc.mark_ready(ReadinessConditions::One);
        assert!(!rc.is_ready());
        assert_eq!(rc.up.get(), 0);

        rc.mark_ready(ReadinessConditions::Two);
        assert!(!rc.is_ready());
        assert_eq!(rc.up.get(), 0);

        rc.mark_ready(ReadinessConditions::Three);
        assert!(rc.is_ready());
        assert_eq!(rc.up.get(), 1);

        rc.mark_not_ready(ReadinessConditions::Two);
        assert!(!rc.is_ready());
        assert_eq!(rc.up.get(), 0);
    }

    #[test]
    fn test_always_ready() {
        let rc = ReadinessProbe::<AlwaysReady>::default();
        assert!(rc.is_ready());
        assert_eq!(rc.up.get(), 1);
    }
}
