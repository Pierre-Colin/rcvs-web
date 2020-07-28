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

impl<V: Serialize + Clone + Eq + Hash + Debug> ResultData<V> {
    pub fn from_election(title: &str, election: &rcvs::Election<V>) -> Option<Self> {
        let graph = election.get_duel_graph();
        let alternatives = graph.get_vertices().to_vec();
        let mut arrows = Vec::new();
        for (f, _) in alternatives.iter().enumerate() {
            for (t, _) in alternatives.iter().enumerate() {
                if graph[(f, t)] {
                    let mut arrow = HashMap::new();
                    arrow.insert(String::from("from"), f);
                    arrow.insert(String::from("to"), t);
                    arrows.push(arrow);
                }
            }
        }
        let strategy = match graph.get_optimal_strategy() {
            Ok(s) => Some(StrategyData::new(&s)),
            Err(_) => None,
        };
        Some(Self {
            title: String::from(title),
            alternatives: alternatives,
            arrows: arrows,
            strategy: strategy,
            winner: None,
        })
    }
}
