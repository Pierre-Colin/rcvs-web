use std::collections::HashMap;
use std::iter::FromIterator;

use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum StrategyData {
    Pure(String),
    Mixed(HashMap<String, f64>),
}

impl StrategyData {
    pub fn new(strategy: &rcvs::Strategy<String>) -> Self {
        match strategy {
            rcvs::Strategy::Pure(a) => Self::Pure(a.to_string()),
            rcvs::Strategy::Mixed(p) => Self::Mixed(HashMap::from_iter(p.iter().cloned())),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ResultData {
    alternatives: Vec<String>,
    arrows: Vec<HashMap<String, String>>,
    strategy: Option<StrategyData>,
    winner: Option<String>,
}

impl ResultData {
    pub fn from_election(election: &rcvs::Election<String>) -> Option<Self> {
        let graph = election.get_duel_graph();
        let alternatives = graph.get_vertices().to_vec();
        let mut arrows = Vec::new();
        for (f, from) in alternatives.iter().enumerate() {
            for (t, to) in alternatives.iter().enumerate() {
                if graph[(f, t)] {
                    let mut arrow = HashMap::new();
                    arrow.insert(String::from("from"), String::from(from));
                    arrow.insert(String::from("to"), String::from(to));
                    arrows.push(arrow);
                }
            }
        }
        let strategy = match graph.get_optimal_strategy() {
            Ok(s) => Some(StrategyData::new(&s)),
            Err(_) => None,
        };
        Some(Self {
            alternatives: alternatives,
            arrows: arrows,
            strategy: strategy,
            winner: None,
        })
    }
}
