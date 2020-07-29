use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::FromIterator;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, Debug)]
pub struct AlternativeData {
    pub id: String,
    pub description: String,
    pub icon: String,
}

#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum StrategyData<V: Serialize + Eq + Hash> {
    Pure(V),
    Mixed(HashMap<V, f64>),
}

impl<V: Serialize + Eq + Hash + Clone> StrategyData<V> {
    pub fn new(strategy: &rcvs::Strategy<V>) -> Self {
        match strategy {
            rcvs::Strategy::Pure(a) => Self::Pure(a.to_owned()),
            rcvs::Strategy::Mixed(p) => Self::Mixed(HashMap::from_iter(p.iter().cloned())),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ResultData<V: Serialize + Eq + Hash> {
    title: String,
    alternatives: Vec<V>,
    arrows: Vec<HashMap<String, usize>>,
    strategy: Option<StrategyData<V>>,
    winner: Option<V>,
}
