use std::collections::HashMap;
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
    title: String,
    alternatives: Vec<String>,
    arrows: Vec<HashMap<String, usize>>,
    strategy: Option<StrategyData>,
    winner: Option<String>,
}

impl ResultData {
    pub fn from_election(title: &str, election: &rcvs::Election<String>) -> Option<Self> {
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
