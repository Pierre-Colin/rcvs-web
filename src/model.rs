use std::{error::Error, fs, path::Path};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct BallotRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elector: Option<usize>,
    pub alternative: usize,
    pub min: u64,
    pub max: u64,
}

#[derive(Serialize, Debug, Clone)]
pub struct AlternativeData {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub icon: String,
}

#[derive(Serialize)]
pub struct ElectionData {
    pub title: Option<String>,
    pub alternatives: Vec<AlternativeData>,
    pub ballot: Vec<BallotRow>,
}

fn connect(url: &str, init: &str) -> rusqlite::Result<Connection> {
    // TODO: fix LBYL data race
    let new = !Path::new(url).exists();
    let connection = Connection::open(url)?;
    if new {
        let model_code = fs::read_to_string(init).expect("Failed to open init code");
        connection
            .execute_batch(&model_code)
            .expect("Failed to run init code");
    }
    Ok(connection)
}

// May be used in the future
#[allow(dead_code)]
pub fn get_ballot(ip: &str) -> Result<Vec<BallotRow>, Box<dyn Error>> {
    let connection = connect("model.db", "model.sql")?;

    let mut statement = connection.prepare(
        "SELECT altId, rankMin, rankMax FROM ranking JOIN elector USING(elecId) WHERE elecIp = ?1",
    )?;
    let mut rows = statement.query(params![ip])?;

    let mut ballot = Vec::new();
    while let Some(row) = rows.next()? {
        ballot.push(BallotRow {
            elector: None,
            alternative: row.get::<usize, i64>(0)? as usize,
            min: row.get::<usize, i64>(1)? as u64,
            max: row.get::<usize, i64>(2)? as u64,
        });
    }
    Ok(ballot)
}

pub fn delete_ballot(ip: &str) -> Result<bool, Box<dyn Error>> {
    let connection = connect("model.db", "model.sql")?;

    let deleted = connection.execute("DELETE FROM elector WHERE elecIp = ?1", params![ip])?;

    Ok(deleted != 0)
}

fn get_elector(ip: &str, connection: &Connection) -> Result<Option<i64>, Box<dyn Error>> {
    let mut statement = connection.prepare("SELECT elecId FROM elector WHERE elecIp = ?1")?;
    let mut rows = statement.query(params![ip])?;

    match rows.next()? {
        Some(row) => Ok(Some(row.get::<usize, i64>(0)?)),
        None => Ok(None),
    }
}

fn get_put_elector(ip: &str, connection: &Connection) -> Result<i64, Box<dyn Error>> {
    if let Some(id) = get_elector(ip, connection)? {
        Ok(id)
    } else {
        connection.execute("INSERT INTO elector VALUES(null, ?1)", params![ip])?;

        match get_elector(ip, connection)? {
            Some(id) => Ok(id),
            None => panic!("Inserting did not work"),
        }
    }
}

pub fn set_ballot(ip: &str, ballot: &[BallotRow]) -> Result<(), Box<dyn Error>> {
    let mut connection = connect("model.db", "model.sql")?;

    let transaction = connection.transaction()?;
    let elector = get_put_elector(ip, &transaction)?;
    transaction.execute("DELETE FROM ranking WHERE elecId = ?1", params![elector])?;
    for row in ballot {
        transaction.execute(
            "INSERT INTO ranking VALUES(?1, ?2, ?3, ?4)",
            params![
                elector,
                row.alternative as i64,
                row.min as i64,
                row.max as i64
            ],
        )?;
    }
    transaction.commit()?;

    Ok(())
}

// TODO: factorize these two functions
pub fn get_data(ip: &str) -> Result<ElectionData, Box<dyn Error>> {
    let mut connection = connect("model.db", "model.sql")?;

    let transaction = connection.transaction()?;
    let mut statement = transaction.prepare("SELECT * FROM alternative")?;
    let alternative_iter = statement.query_map(params![], |row| {
        Ok(AlternativeData {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row
                .get::<usize, Option<String>>(2)?
                .unwrap_or(String::new()),
            icon: row
                .get::<usize, Option<String>>(3)?
                .unwrap_or(String::new()),
        })
    })?;

    let mut alternatives = Vec::new();
    for alternative in alternative_iter {
        alternatives.push(alternative?);
    }
    std::mem::drop(statement);

    let mut statement = transaction.prepare(
        "SELECT altId, rankMin, rankMax FROM ranking JOIN elector USING(elecId) WHERE elecIp = ?1",
    )?;
    let ballot_iter = statement.query_map(params![ip], |row| {
        Ok(BallotRow {
            elector: None,
            alternative: row.get::<usize, i64>(0)? as usize,
            min: row.get::<usize, i64>(1)? as u64,
            max: row.get::<usize, i64>(2)? as u64,
        })
    })?;

    let mut ballot = Vec::new();
    for row in ballot_iter {
        ballot.push(row?);
    }

    Ok(ElectionData {
        title: None,
        alternatives: alternatives,
        ballot: ballot,
    })
}

pub fn collect_votes() -> Result<ElectionData, Box<dyn Error>> {
    let mut connection = connect("model.db", "model.sql")?;

    let transaction = connection.transaction()?;
    let mut statement = transaction.prepare("SELECT * FROM alternative")?;
    let alternative_iter = statement.query_map(params![], |row| {
        Ok(AlternativeData {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row
                .get::<usize, Option<String>>(2)?
                .unwrap_or(String::new()),
            icon: row
                .get::<usize, Option<String>>(3)?
                .unwrap_or(String::new()),
        })
    })?;

    let mut alternatives = Vec::new();
    for alternative in alternative_iter {
        alternatives.push(alternative?);
    }
    std::mem::drop(statement);

    let mut statement = transaction.prepare("SELECT * FROM ranking")?;
    let ballot_iter = statement.query_map(params![], |row| {
        Ok(BallotRow {
            elector: Some(row.get::<usize, i64>(0)? as usize),
            alternative: row.get::<usize, i64>(1)? as usize,
            min: row.get::<usize, i64>(2)? as u64,
            max: row.get::<usize, i64>(3)? as u64,
        })
    })?;

    let mut ballot = Vec::new();
    for row in ballot_iter {
        ballot.push(row?);
    }

    Ok(ElectionData {
        title: None,
        alternatives: alternatives,
        ballot: ballot,
    })
}
